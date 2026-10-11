#!/bin/bash
#
# LLM Services - Start/Stop embedding and chat services
#
# Backend selection (per service, in this order):
#   1. $LLAMA_SERVER / $RMI_BIN            explicit override
#   2. llama-server found on PATH or in ./bin
#   3. ./bin/rust-model-inference           fallback
#
# llama-server is preferred because it is 3-4x faster on this box for
# LFM2.5-8B-A1B (measured: 14.9 vs 4.5 tok/s generation, 39.4 vs 10.2 tok/s
# prompt on 12 threads, Q8_0). rust-model-inference stays as the fallback so the
# stack still starts when llama.cpp is not built.
#
# Force a backend per service with the environment:
#   LLM_BACKEND=llama|rmi        ./serve.sh start
#   EMBEDDING_BACKEND=llama|rmi  ./serve.sh start
#   auto (default) picks llama-server when available, else rust-model-inference.
#
# Chat context size is read from the project .env (MAX_CONTEXT_TOKENS) so the
# server window and the backend's compression budget stay in sync; override with
# LLM_CTX.
#
# Usage: ./LLMServices/serve.sh {install|start|stop|status|restart|help}
#   ./serve.sh start    - Start both services in background
#   ./serve.sh stop     - Stop both services
#   ./serve.sh status   - Check if services are running
#   ./serve.sh restart  - Restart both services
#

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
LOG_DIR="$SCRIPT_DIR/logs"
PID_DIR="$SCRIPT_DIR/logs"
LOCAL_BIN="$SCRIPT_DIR/bin"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

mkdir -p "$LOG_DIR"

# Read a KEY=value from the project .env without sourcing the whole file
# (sourcing would execute arbitrary shell, and the env file is not ours).
env_value() {
    local key="$1" default="$2" file
    for file in "$PROJECT_ROOT/.env" "$SCRIPT_DIR/.env"; do
        [ -f "$file" ] || continue
        local line
        line="$(grep -E "^[[:space:]]*${key}=" "$file" 2>/dev/null | tail -n 1 || true)"
        [ -n "$line" ] || continue
        line="${line#*=}"
        # strip surrounding whitespace, inline comments and quotes
        line="${line%%#*}"
        line="$(echo "$line" | sed -e 's/^[[:space:]]*//' -e 's/[[:space:]]*$//' \
                                 -e 's/^"\(.*\)"$/\1/' -e "s/^'\(.*\)'\$/\1/")"
        if [ -n "$line" ]; then
            echo "$line"
            return 0
        fi
    done
    echo "$default"
}

# LFM2.5-Embedding-350M. ~3x faster than Qwen3-0.6B, but its Chinese vectors
# are weakly discriminative: "诸葛亮" scores closer to "孙悟空" (0.33) than to
# its own alias "孔明" (0.47). Known limitation -- semantic ranking is coarse on
# Chinese, so keyword/FTS has to carry most of the precision on this corpus.
EMBEDDING_MODEL="$SCRIPT_DIR/models/embedding/LFM2.5-Embedding-350M-Q8_0.gguf"
EMBEDDING_PORT=9011
EMBEDDING_PID_FILE="$PID_DIR/embedding.pid"
EMBEDDING_LOG="$LOG_DIR/embedding.log"

# Q4_K_M first: on this box it decodes ~39% faster than Q8_0 (30.1 vs 21.6 t/s
# at 8 threads) because token generation is memory-bandwidth bound and Q4_K_M
# halves the bytes streamed per token. Prompt processing is the opposite --
# Q8_0 + KLEIDI reaches ~135 t/s vs ~80 t/s for Q4_K_M -- so long-context
# callers should override with LLM_MODEL=.../LFM2.5-8B-A1B-Q8_0.gguf.
pick_llm_model() {
    local candidate
    for candidate in \
        "$SCRIPT_DIR/models/llm/LFM2.5-8B-A1B-UD-Q4_K_M.gguf" \
        "$SCRIPT_DIR/models/llm/LFM2.5-8B-A1B-Q8_0.gguf"
    do
        if [ -f "$candidate" ]; then
            echo "$candidate"
            return 0
        fi
    done
    echo ""
}

LLM_MODEL="${LLM_MODEL:-$(pick_llm_model)}"
if [ -z "$LLM_MODEL" ]; then
    echo "error: no LFM2.5-8B-A1B gguf found under $SCRIPT_DIR/models/llm/." >&2
    exit 1
fi
LLM_PID_FILE="$PID_DIR/llm.pid"
LLM_LOG="$LOG_DIR/llm.log"
LLM_PORT=9010

THREADS="${LLM_THREADS:-8}"

# Context window for the chat service. Kept in lockstep with the backend's
# MAX_CONTEXT_TOKENS so the server never rejects a request the backend already
# considered in-budget. Override with LLM_CTX, or just edit MAX_CONTEXT_TOKENS
# in the project .env.
CTX="${LLM_CTX:-$(env_value MAX_CONTEXT_TOKENS 8192)}"
case "$CTX" in
    ''|*[!0-9]*)
        echo "warning: invalid context size '$CTX', falling back to 8192" >&2
        CTX=8192
        ;;
esac

# Number of concurrent request slots. llama-server with a single slot answers
# every extra request with 429/503 once the slot is busy, which happens as soon
# as chat overlaps with the indexer's digest run or a settings connection test.
# CTX is the *per slot* budget here, so the total passed to --ctx-size is scaled
# by this factor; otherwise raising PARALLEL would silently shrink every
# conversation's context to CTX/PARALLEL.
PARALLEL="${LLM_PARALLEL:-2}"
case "$PARALLEL" in
    ''|*[!0-9]*|0)
        echo "warning: invalid LLM_PARALLEL '$PARALLEL', falling back to 2" >&2
        PARALLEL=2
        ;;
esac
TOTAL_CTX=$((CTX * PARALLEL))

# ---------------------------------------------------------------------------
# Backend resolution
# ---------------------------------------------------------------------------

# Absolute path of the llama-server binary, or empty when there is none.
#
# Searched in the order a user would expect: an explicit $LLAMA_SERVER, the
# in-tree ./bin (the supported location — see `serve.sh install`), then PATH.
#
# LLMServices/bin is gitignored, so the binary is deliberately not tracked; a
# checkout that has never run `install` will fall through to PATH and fail with
# the message below, rather than silently picking up a build out of some
# unrelated directory.
find_llama_server() {
    local candidate
    if [ -n "${LLAMA_SERVER:-}" ] && [ -x "$LLAMA_SERVER" ]; then
        echo "$LLAMA_SERVER"
        return 0
    fi
    for candidate in \
        "$LOCAL_BIN/llama-server" \
        "$(command -v llama-server 2>/dev/null || true)"
    do
        [ -n "$candidate" ] && [ -x "$candidate" ] && { echo "$candidate"; return 0; }
    done
    return 1
}

RMI_BIN_DEFAULT="$LOCAL_BIN/rust-model-inference"

# Absolute path of the rust-model-inference binary, or empty.
find_rmi_bin() {
    local candidate
    for candidate in "${RMI_BIN:-}" "$RMI_BIN_DEFAULT" "$(command -v rust-model-inference 2>/dev/null || true)"; do
        [ -n "$candidate" ] && [ -x "$candidate" ] && { echo "$candidate"; return 0; }
    done
    return 1
}

# Resolve one service's backend. Sets BACKEND_KIND and BACKEND_BIN.
#
# $1 = "llama" | "rmi" | "auto"
#
# `auto` prefers llama-server. An explicit request that cannot be satisfied is an
# error rather than a silent downgrade: asking for `llama` and getting rmi would
# make a 4x latency regression look like a working start.
resolve_backend() {
    local want="$1"
    BACKEND_KIND=""
    BACKEND_BIN=""
    local llama rmi
    llama="$(find_llama_server || true)"
    rmi="$(find_rmi_bin || true)"

    case "$want" in
        llama)
            if [ -z "$llama" ]; then
                echo "error: LLM_BACKEND/EMBEDDING_BACKEND=llama but no llama-server found." >&2
                echo "       Set \$LLAMA_SERVER, put one in $LOCAL_BIN, or use 'auto'." >&2
                return 1
            fi
            BACKEND_KIND=llama; BACKEND_BIN="$llama"
            ;;
        rmi)
            if [ -z "$rmi" ]; then
                echo "error: backend rmi requested but no rust-model-inference found." >&2
                return 1
            fi
            BACKEND_KIND=rmi; BACKEND_BIN="$rmi"
            ;;
        auto|"")
            if [ -n "$llama" ]; then
                BACKEND_KIND=llama; BACKEND_BIN="$llama"
            elif [ -n "$rmi" ]; then
                BACKEND_KIND=rmi; BACKEND_BIN="$rmi"
            else
                echo "error: neither llama-server nor rust-model-inference found." >&2
                return 1
            fi
            ;;
        *)
            echo "error: unknown backend '$want' (expected llama, rmi, or auto)." >&2
            return 1
            ;;
    esac
}

# Build the argv for one service under the resolved backend.
#
# llama-server and rust-model-inference take different flags for the same
# intent, and the embedding service needs pooling flags the LLM one does not.
#
#   $1 = backend kind (llama|rmi)
#   $2 = model path
#   $3 = port
#   $4 = "embedding" | "chat"
service_argv() {
    local kind="$1" model="$2" port="$3" mode="$4"
    if [ "$kind" = llama ]; then
        if [ "$mode" = embedding ]; then
            # Qwen3-Embedding is a decoder with last-token pooling. Without
            # --pooling last llama-server pools by mean over a causal model and
            # returns a wrong-length, wrong-norm vector; --embeddings is what
            # switches the server into embedding mode.
            # --parallel 2 so a search's query embedding is not stuck behind an
            # in-flight indexing batch: with a single slot the query waits for
            # the whole batch to finish.
            echo "--model $model --host 0.0.0.0 --port $port --threads $THREADS --pooling last --embeddings --parallel 2"
        else
            # --jinja is llama-server's default, and it is what makes the server
            # render the GGUF's own tokenizer.chat_template. Turning it off would
            # fall back to a generic role\ncontent\n form. --parallel gives us
            # more than one in-flight request, and --ctx-size is the total shared
            # across those slots (CTX * PARALLEL), keeping CTX per conversation.
            echo "--model $model --host 0.0.0.0 --port $port --threads $THREADS -c $TOTAL_CTX --parallel $PARALLEL"
        fi
    else
        if [ "$mode" = embedding ]; then
            echo "--serve --model $model --embedding --host 0.0.0.0 --port $port --threads $THREADS"
        else
            echo "--serve --model $model --host 0.0.0.0 --port $port --threads $THREADS --max-context $CTX"
        fi
    fi
}

# ---------------------------------------------------------------------------
# Service lifecycle
# ---------------------------------------------------------------------------

start_service() {
    local name="$1" pid_file="$2" log_file="$3" backend="$4" model="$5" port="$6" mode="$7"

    if [ -f "$pid_file" ] && kill -0 "$(cat "$pid_file")" 2>/dev/null; then
        echo "[$name] Already running (PID $(cat "$pid_file"), $backend)"
        return 0
    fi

    if [ ! -f "$model" ]; then
        echo "[$name] Model not found: $model"
        return 1
    fi

    resolve_backend "$backend"
    local argv
    argv="$(service_argv "$BACKEND_KIND" "$model" "$port" "$mode")"

    echo "[$name] Starting ($BACKEND_KIND)..."
    # Word splitting is intended here: argv is a command line, and every
    # element is either a path this script built or a literal flag.
    # shellcheck disable=SC2086
    setsid nohup "$BACKEND_BIN" $argv > "$log_file" 2>&1 < /dev/null &
    local pid=$!
    echo "$pid" > "$pid_file"

    # Poll for the port rather than sleeping a fixed 3s: an 8B model takes ~15s
    # to page in from disk on a cold cache, and a fixed sleep reported success
    # for processes that then died during load.
    local waited=0
    while [ "$waited" -lt 120 ]; do
        if ! kill -0 "$pid" 2>/dev/null; then
            echo "[$name] Failed to start (exited during load). Check $log_file"
            rm -f "$pid_file"
            return 1
        fi
        # Readiness differs per backend: llama-server exposes /health and answers
        # 503 until the model is paged in (hence -f, otherwise curl exits 0 on the
        # 503 and the script would claim success too early). rust-model-inference
        # has no /health, so for rmi a listening socket is the strongest signal
        # available.
        if [ "$BACKEND_KIND" = llama ]; then
            if command -v curl > /dev/null 2>&1; then
                if curl -sf -m 2 -o /dev/null "http://127.0.0.1:$port/health" 2>/dev/null; then
                    echo "[$name] Started (PID $pid, port $port, $BACKEND_KIND, ctx $CTX)"
                    return 0
                fi
            elif (exec 3<>"/dev/tcp/127.0.0.1/$port") 2> /dev/null; then
                echo "[$name] Started (PID $pid, port $port, $BACKEND_KIND, ctx $CTX)"
                return 0
            fi
        else
            if (exec 3<>"/dev/tcp/127.0.0.1/$port") 2> /dev/null; then
                echo "[$name] Started (PID $pid, port $port, $BACKEND_KIND, ctx $CTX)"
                return 0
            fi
        fi
        sleep 1
        waited=$((waited + 1))
    done

    echo "[$name] Timed out after ${waited}s waiting for port $port. Check $log_file"
    rm -f "$pid_file"
    return 1
}

stop_service() {
    local name="$1" pid_file="$2"

    if [ ! -f "$pid_file" ]; then
        echo "[$name] Not running"
        return 0
    fi

    local pid
    pid=$(cat "$pid_file")
    if kill -0 "$pid" 2>/dev/null; then
        echo "[$name] Stopping (PID $pid)..."
        # setsid put the server in its own process group, so signal the group:
        # llama-server spawns a thread pool and a single SIGTERM to the parent
        # can leave the listener holding the port.
        kill -TERM "-$pid" 2> /dev/null || kill "$pid" 2> /dev/null || true
        local waited=0
        while kill -0 "$pid" 2> /dev/null && [ "$waited" -lt 15 ]; do
            sleep 1
            waited=$((waited + 1))
        done
        if kill -0 "$pid" 2>/dev/null; then
            echo "[$name] Force killing..."
            kill -9 "-$pid" 2> /dev/null || kill -9 "$pid" 2> /dev/null || true
        fi
        echo "[$name] Stopped"
    else
        echo "[$name] Process not found, cleaning pid file"
    fi
    rm -f "$pid_file"
}

status_service() {
    local name="$1" pid_file="$2" port="$3"
    if [ -f "$pid_file" ] && kill -0 "$(cat "$pid_file")" 2>/dev/null; then
        echo "[$name] Running (PID $(cat "$pid_file"), port $port)"
    else
        echo "[$name] Not running"
    fi
}

show_backends() {
    local llama rmi
    llama="$(find_llama_server || true)"
    rmi="$(find_rmi_bin || true)"
    echo "  llama-server:        ${llama:-<not found>}"
    echo "  rust-model-inference: ${rmi:-<not found>}"
    echo "  chat context:        $CTX (MAX_CONTEXT_TOKENS from .env, override with LLM_CTX)"
    echo "  parallel slots:      $PARALLEL (total ctx $TOTAL_CTX, override with LLM_PARALLEL)"
    echo "  threads:             $THREADS"
}

case "${1:-}" in
    install)
        # Copy a llama.cpp build into LLMServices/bin so the project is
        # self-contained instead of reaching into an unrelated checkout.
        # Usage: serve.sh install [path/to/llama.cpp/build/bin]
        src="${2:-$HOME/Repos/rust-model-inference/references/llama.cpp/build/bin}"
        if [ ! -x "$src/llama-server" ]; then
            echo "error: no llama-server under '$src'." >&2
            echo "usage: $0 install [path/to/llama.cpp/build/bin]" >&2
            exit 1
        fi
        mkdir -p "$LOCAL_BIN"
        # The shared libraries travel with the binary; llama-server resolves
        # them from its own directory via rpath.
        cp -a "$src/llama-server" "$src/libllama"*.so* "$src/libggml"*.so* "$LOCAL_BIN"/ 2>/dev/null
        [ -x "$src/llama-bench" ] && cp -a "$src/llama-bench" "$LOCAL_BIN"/
        echo "installed into $LOCAL_BIN"
        "$LOCAL_BIN/llama-server" --version 2>/dev/null | head -1
        ;;
    start)
        echo "=== Starting LLM Services ==="
        show_backends
        echo ""
        LLM_BACKEND="${LLM_BACKEND:-auto}"
        EMBEDDING_BACKEND="${EMBEDDING_BACKEND:-auto}"

        start_service "LLM" "$LLM_PID_FILE" "$LLM_LOG" \
            "$LLM_BACKEND" "$LLM_MODEL" "$LLM_PORT" chat || true
        start_service "Embedding" "$EMBEDDING_PID_FILE" "$EMBEDDING_LOG" \
            "$EMBEDDING_BACKEND" "$EMBEDDING_MODEL" "$EMBEDDING_PORT" embedding || true

        echo ""
        echo "=== Services ==="
        echo "  Embedding API: http://127.0.0.1:$EMBEDDING_PORT/v1/embeddings"
        echo "  Chat API:      http://127.0.0.1:$LLM_PORT/v1/chat/completions"
        echo "  Models API:    http://127.0.0.1:$LLM_PORT/v1/models"
        echo ""
        echo "Logs: $LOG_DIR/"
        ;;
    stop)
        echo "=== Stopping LLM Services ==="
        stop_service "LLM" "$LLM_PID_FILE"
        stop_service "Embedding" "$EMBEDDING_PID_FILE"
        ;;
    status)
        echo "=== LLM Services Status ==="
        status_service "Embedding" "$EMBEDDING_PID_FILE" "$EMBEDDING_PORT"
        status_service "LLM" "$LLM_PID_FILE" "$LLM_PORT"
        echo ""
        show_backends
        ;;
    restart)
        "$0" stop
        sleep 2
        "$0" start
        ;;
    help|--help|-h)
        echo "LLM Services - Manage embedding and chat API services"
        echo ""
        echo "Usage: $0 {install|start|stop|status|restart|help}"
        echo ""
        echo "Commands:"
        echo "  install   Copy a llama.cpp build into LLMServices/bin (self-contained)"
        echo "  start     Start embedding + chat services in background"
        echo "  stop     Stop both services"
        echo "  status   Show running status and which backends were found"
        echo "  restart  Restart both services"
        echo "  help     Show this help message"
        echo ""
        echo "Services:"
        echo "  Chat       http://127.0.0.1:$LLM_PORT/v1/chat/completions"
        echo "  Embedding  http://127.0.0.1:$EMBEDDING_PORT/v1/embeddings"
        echo "  Models     http://127.0.0.1:$LLM_PORT/v1/models"
        echo ""
        echo "Backends (per service, via environment):"
        echo "  LLM_BACKEND=auto|llama|rmi        chat service"
        echo "  EMBEDDING_BACKEND=auto|llama|rmi  embedding service"
        echo "  auto (default) prefers llama-server, falls back to rust-model-inference."
        echo ""
        echo "  LLAMA_SERVER=/path/to/llama-server   override the search"
        echo "  RMI_BIN=/path/to/rust-model-inference"
        echo ""
        echo "Tuning:"
        echo "  LLM_CTX=32768        chat context window (default: MAX_CONTEXT_TOKENS in .env)"
        echo "  LLM_PARALLEL=2       concurrent request slots; ctx total is LLM_CTX*PARALLEL"
        echo "  LLM_THREADS=8        inference threads"
        echo ""
        echo "Logs: $LOG_DIR/"
        ;;
    *)
        echo "Usage: $0 {install|start|stop|status|restart|help}"
        echo "Run '$0 help' for details."
        exit 1
        ;;
esac