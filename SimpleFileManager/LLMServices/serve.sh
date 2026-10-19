#!/bin/bash
#
# LLM Services - Start/Stop embedding and chat services
#
# Backend selection (per service, in this order):
#   1. $LLAMA_SERVER / $RMI_BIN            explicit override
#   2. llama-server found on PATH or in ./bin
#   3. ./bin/rust-model-inference           fallback
#
# llama-server is preferred because it is ~2-3x faster on this box than
# rust-model-inference for the chat workload (LFM2.5-24B-A2B Q4_K_M on the
# CIX CD8180: pp=63 t/s, tg=21 t/s at 8 threads on the A720 big cores via
# KleidiAI). rust-model-inference stays as the fallback so the stack still
# starts when llama.cpp is not built.
#
# Per-service threading & CPU pinning: LLM_THREADS/EMB_THREADS govern how many
# threads each service opens, and LLM_CPUSET/EMB_CPUSET pin them to specific
# logical CPUs (hex bitmask). When unset, both masks are auto-detected from
# /sys/devices/system/cpu/cpuN/cpu_capacity: highest-capacity tier -> LLM,
# everything else -> embedding. On a homogeneous CPU both stay empty and
# taskset is skipped.
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

# BGE-base-zh-v1.5 Q4_0 (102M params, 768-dim, ~61 MB). Chosen over
# Qwen3-Embedding-0.6B: BGE is Chinese-only and trained for retrieval, so for
# the four 古典小说 in ./backend/data it scores competitively on CMTEB while
# being ~6x smaller. Q4_0 chosen over Q8_0 on a measured 102M-pair semantic
# test (max Δ < 0.025 across synonyms, aliases, unrelated pairs) -- the
# KleidiAI SDOT path on A520 LITTLE picks up the int4 matmul for ~1.2x faster
# batches and the smaller weight table cuts memory bandwidth by 40% during
# 9500-chunk indexer sweeps. The Q8_0 file is kept alongside as a fallback.
EMBEDDING_MODEL="$SCRIPT_DIR/models/bge-base-zh-v1.5/bge-base-zh-v1.5.Q4_0.gguf"
EMBEDDING_PORT=9011
EMBEDDING_PID_FILE="$PID_DIR/embedding.pid"
EMBEDDING_LOG="$LOG_DIR/embedding.log"
# BGE uses CLS-token pooling. Override with `last` / `mean` for other
# embedding backends (e.g. Qwen3 wants `last`).
EMBEDDING_POOLING="${EMBEDDING_POOLING:-cls}"
# Parallel slots for the embedding server. More slots mean simultaneous
# requests (e.g. search while indexer is running) don't queue behind each
# other. BGE-base-zh is 102M params and 768-dim: per-slot KV cache is ~30 KiB,
# so 8 slots cost essentially zero memory. Set lower if you want to throttle.
EMBEDDING_PARALLEL="${EMBEDDING_PARALLEL:-8}"

# LFM2.5-24B-A2B Q4_K_M. 24B with 2.45B active per token -- MoE routing keeps
# the per-token compute at the small-model tier while the 13.5 GiB weights
# give the slow CPU more than enough room to read from RAM. Measured on
# CIX CD8180 (12-core ARMv9, -march=armv9-a, KleidiAI): pp512=63 t/s,
# tg64=21 t/s at LLM_THREADS=8 on the A720 big cores.
pick_llm_model() {
    local candidate
    for candidate in \
        "$SCRIPT_DIR/models/lfm25-24b-a2b-q4km/LFM2.5-24B-A2B-Q4_K_M.gguf"
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
    echo "error: no LFM2.5-24B-A2B gguf found under $SCRIPT_DIR/models/lfm25-24b-a2b-q4km/." >&2
    exit 1
fi
LLM_PID_FILE="$PID_DIR/llm.pid"
LLM_LOG="$LOG_DIR/llm.log"
LLM_PORT=9010

# Fast-path LLM: a small (350M-class) model that answers simple questions in
# under a second. Optional -- skipped silently when the gguf is absent so a
# fresh checkout does not error on `serve.sh start`. The backend's
# /api/agent/chat/stream reads LLM_FAST_BASE_URL from the project .env and
# routes per-turn use_fast requests here. Pinning to the A520 LITTLE cores
# keeps the fast path off the A720 big cores where the 24B is running, so
# the two workloads never contend for the same cache lines.
#
# Disabled by default in this repo -- it pulls in a ~220 MB gguf and
# reuses the embedding server's cpuset pattern. Set LLM_FAST_ENABLED=true
# after dropping the file in models/lfm25-350m-q4_0/ to bring it up.
# env_value() reads from the project .env so this works the same way as
# MAX_CONTEXT_TOKENS / KV_CACHE_TYPE — the operator sets it once, the
# next `serve.sh start` brings the 350M up on 9013. Env override at the
# call site (LLM_FAST_ENABLED=true ./serve.sh start) still wins.
LLM_FAST_ENABLED="${LLM_FAST_ENABLED:-$(env_value LLM_FAST_ENABLED false)}"
pick_llm_fast_model() {
    local candidate
    for candidate in \
        "$SCRIPT_DIR/models/lfm25-350m-q4_0/LFM2.5-350M-Q4_0.gguf"
    do
        if [ -f "$candidate" ]; then
            echo "$candidate"
            return 0
        fi
    done
    echo ""
}
LLM_FAST_MODEL="${LLM_FAST_MODEL:-$(pick_llm_fast_model)}"
LLM_FAST_PID_FILE="$PID_DIR/llm-fast.pid"
LLM_FAST_LOG="$LOG_DIR/llm-fast.log"
LLM_FAST_PORT=9013
# Cheap model is decode-bound on a few cores; reusing EMB_CPUSET keeps
# the existing big.LITTLE topology map honest (chat on big, everything
# else on little).
LLM_FAST_CPUSET="${LLM_FAST_CPUSET:-$EMB_CPUSET}"

# KV cache quantisation. f16 (default) is exact; q8_0 halves the bytes/token
# in the K and V caches (from 80 KiB/tok to 40 KiB/tok on LFM2.5-24B), which
# for our 32K*2-slot config drops KV memory from 5.0 GiB to 2.5 GiB. Quality
# regression is small enough that llama-bench can't see it; the saved memory
# is what kept the previous 30 GiB config from sliding into OOM under bursty
# load. Override with KV_CACHE_TYPE=q4_0 if you also want the V half quantised.
KV_CACHE_TYPE="${KV_CACHE_TYPE:-q8_0}"

# Per-service thread count. The two services share one CPU, so we give the LLM
# most of it and leave the embedding a small slice -- the 0.6B/350M embedding is
# decode-bound on a handful of cores, while a 24B/8B LLM scales into 8 threads
# before saturating memory bandwidth. Override either via the corresponding env
# var.
LLM_THREADS="${LLM_THREADS:-8}"
EMB_THREADS="${EMB_THREADS:-3}"

# CPU pinning masks, hex bitmask of allowed cores (one bit per logical CPU).
# Auto-detected below from /sys/.../cpu_capacity, where the kernel reports a
# "performance tier" -- identical on homogeneous CPUs, distinct on big.LITTLE.
# On a 12-core CD8180 this resolves to big=0xFC3 (CPU 0,1,6-11 = 8x A720) and
# little=0x3C (CPU 2-5 = 4x A520). On a homogeneous i7/x86 box both stay empty
# and we let the kernel EAS decide.
LLM_CPUSET="${LLM_CPUSET:-}"
EMB_CPUSET="${EMB_CPUSET:-}"

# detect_cpu_topology populates LLM_CPUSET / EMB_CPUSET from /sys when they
# weren't set by the caller. Falls back to empty strings (= no pinning) on:
#   - homogeneous CPUs (one capacity tier); pinning would do nothing useful
#   - environments without /sys/cpu_capacity (macOS, BSD, etc.)
#   - CPUs smaller than a 2-tier split (e.g. single-core dev board)
#
# big vs little is decided by ratio, not by absolute max. The kernel reports
# every uarch binning tier its own capacity (A720 prime=1024, mid=905/866,
# A720 lower=984, A520=279 on the CD8180), but for pinning we only care
# whether a tier is meaningfully slower than the others. A threshold of 60% of
# the max capacity captures the A720/A520 split (A520 ~0.27 of max) without
# needlessly splitting the A720 sub-tiers -- an A720 mid-bin core is still
# 3x faster than the A520 little core, which is what matters here.
detect_cpu_topology() {
    [ -d /sys/devices/system/cpu ] || return 0
    [ -z "$LLM_CPUSET" ] || [ -z "$EMB_CPUSET" ] || return 0

    local ncpu=0
    for _f in /sys/devices/system/cpu/cpu*/cpu_capacity; do
        [ -r "$_f" ] || continue
        ncpu=$((ncpu + 1))
    done
    [ "$ncpu" -ge 4 ] || return 0

    local caps=""
    local i=0
    local cap
    # `sort -V` orders cpu0, cpu1, ..., cpu10, cpu11 the way the kernel numbers
    # them; the alternative of reading /proc/cpuinfo is slower and breaks on
    # some embedded kernels.
    for cap in $(ls /sys/devices/system/cpu/cpu*/cpu_capacity 2>/dev/null \
                 | sort -V | xargs -I{} cat {} 2>/dev/null); do
        caps="$caps $cap"
        i=$((i + 1))
    done
    [ "$i" -eq "$ncpu" ] || return 0

    local max_cap min_cap
    max_cap=$(echo "$caps" | tr ' ' '\n' | grep -v '^$' | sort -n | tail -1)
    min_cap=$(echo "$caps" | tr ' ' '\n' | grep -v '^$' | sort -n | head -1)
    # Compute the cutoff as 60% of max -- this is the A520 boundary on
    # CD8180 (279 / 1024 = 0.27) and stays well clear of any A720 sub-tier.
    local cutoff=$(( (max_cap * 60) / 100 ))
    [ "$cutoff" -gt 0 ] || return 0

    local n_big=0 n_little=0
    local big_mask=0 little_mask=0
    local idx=0
    for cap in $caps; do
        if [ "$cap" -ge "$cutoff" ]; then
            big_mask=$((big_mask | (1 << idx)))
            n_big=$((n_big + 1))
        else
            little_mask=$((little_mask | (1 << idx)))
            n_little=$((n_little + 1))
        fi
        idx=$((idx + 1))
    done
    # Require *both* tiers, with at least one core each, to bother pinning.
    # otherwise this is a homogeneous box, where pinning would only reduce
    # the scheduler's flexibility.
    if [ "$n_little" -lt 1 ] || [ "$n_big" -lt 1 ] \
       || [ $((n_big + n_little)) -ne "$ncpu" ]; then
        return 0
    fi

    [ -z "$LLM_CPUSET" ]  && LLM_CPUSET=$(printf '%X' "$big_mask")
    [ -z "$EMB_CPUSET" ]  && EMB_CPUSET=$(printf '%X' "$little_mask")
}

# Run the detector so the rest of the script can treat the variables as
# already populated; cheaper than calling it in every branch.
detect_cpu_topology

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
    local svc_threads
    if [ "$mode" = embedding ]; then
        svc_threads="$EMB_THREADS"
    elif [ "$mode" = "chat-fast" ]; then
        # 350M is decode-bound on ~3 cores on the A520 LITTLE tier; no
        # point opening 8 worker threads.
        svc_threads="${LLM_FAST_THREADS:-3}"
    else
        svc_threads="$LLM_THREADS"
    fi
    if [ "$kind" = llama ]; then
        if [ "$mode" = embedding ]; then
            # BGE-family models use CLS pooling; Qwen3-Embedding uses last.
            # Override via EMBEDDING_POOLING. --embeddings switches the
            # server into embedding mode; without it /v1/embeddings returns
            # 404. EMBEDDING_PARALLEL slots (default 8) lets a search query
            # run alongside an in-flight indexing batch: with --parallel 2
            # the query queues ~30s waiting for a slot; with 8 slots it slips
            # in immediately. Per-slot cost is ~30 KiB KV cache for BGE-base-zh
            # so 8 slots is essentially free.
            echo "--model $model --host 0.0.0.0 --port $port --threads $svc_threads --pooling $EMBEDDING_POOLING --embeddings --parallel $EMBEDDING_PARALLEL"
        elif [ "$mode" = "chat-fast" ]; then
            # Fast-path chat. Smaller context (8K is plenty for short Q&A)
            # and a single parallel slot since the 350M is decode-bound on
            # 3-4 cores; an extra slot would just thrash the LITTLE caches.
            # No -ctk/-ctv override: the 350M is small enough that f16
            # KV costs ~6 MiB/slot, well under 1% of total memory.
            echo "--model $model --host 0.0.0.0 --port $port --threads $svc_threads -c 8192 --parallel 1"
        else
            # --jinja is llama-server's default, and it is what makes the server
            # render the GGUF's own tokenizer.chat_template. Turning it off would
            # fall back to a generic role\ncontent\n form. --parallel gives us
            # more than one in-flight request, and --ctx-size is the total shared
            # across those slots (CTX * PARALLEL), keeping CTX per conversation.
            # -ctk/-ctv quantise the K/V caches (q8_0 halves the per-slot memory
            # for negligible quality loss; raising the type to f16 trades 2.5 GiB
            # back for a hair more accuracy).
            echo "--model $model --host 0.0.0.0 --port $port --threads $svc_threads -c $TOTAL_CTX --parallel $PARALLEL -ctk $KV_CACHE_TYPE -ctv $KV_CACHE_TYPE"
        fi
    else
        if [ "$mode" = embedding ]; then
            echo "--serve --model $model --embedding --host 0.0.0.0 --port $port --threads $svc_threads"
        else
            echo "--serve --model $model --host 0.0.0.0 --port $port --threads $svc_threads --max-context $CTX"
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

    # Pick the cpuset for this service. Pinning keeps the LLM out of the
    # embedding server's hot path and keeps chat latency from jittering when
    # the embedding's burst wakes idle worker threads on a different tier.
    # The fast-path chat uses the embedding's cpuset (A520 LITTLE) by
    # default so it does not fight the 24B on the big cores. Homogeneous
    # machines leave one of these empty, which the guarded taskset below
    # translates to "no pinning".
    local cpuset=""
    if [ "$mode" = embedding ]; then
        cpuset="$EMB_CPUSET"
    elif [ "$mode" = "chat-fast" ]; then
        cpuset="$LLM_FAST_CPUSET"
    else
        cpuset="$LLM_CPUSET"
    fi

    echo "[$name] Starting ($BACKEND_KIND, cpuset=$([ -n "$cpuset" ] && echo "0x$cpuset" || echo "<unset>"))..."
    # Word splitting is intended here: argv is a command line, and every
    # element is either a path this script built or a literal flag.
    # shellcheck disable=SC2086
    if [ -n "$cpuset" ]; then
        # taskset -a sets affinity on every thread of the calling process; the
        # llama-server worker threads inherit it after exec, so pinning
        # happens before any worker materialises.
        setsid nohup taskset -a "$cpuset" "$BACKEND_BIN" $argv > "$log_file" 2>&1 < /dev/null &
    else
        setsid nohup "$BACKEND_BIN" $argv > "$log_file" 2>&1 < /dev/null &
    fi
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
    echo "  KV cache type:       $KV_CACHE_TYPE  (per-slot: 2.5 GiB at q8_0, 5.0 GiB at f16 for 32K ctx)"
    if [ -n "$LLM_CPUSET" ] && [ -n "$EMB_CPUSET" ]; then
        echo "  cpu pinning:         big.LITTLE detected -- LLM=0x$LLM_CPUSET, embed=0x$EMB_CPUSET"
    else
        echo "  cpu pinning:         none (homogeneous CPU or /sys/cpu_capacity absent)"
    fi
    echo "  LLM threads:         $LLM_THREADS   | embedding threads: $EMB_THREADS"
}

# ---------------------------------------------------------------------------
# GPU embedding server (search-dedicated)
# ---------------------------------------------------------------------------
# Optional second llama-server on the Mali-G720 Vulkan device, used by the
# backend for low-latency user queries. CPU LITTLE (port 9011) stays as
# the indexer's batched workhorse; this one serves the search path
# exclusively. The backend's EMBEDDING_SEARCH_URL is expected to point at
# $GPU_EMBEDDING_PORT.

GPU_EMBEDDING_PORT="${GPU_EMBEDDING_PORT:-9012}"
GPU_EMBEDDING_PID_FILE="$PID_DIR/embedding-gpu.pid"
GPU_EMBEDDING_LOG="$LOG_DIR/embedding-gpu.log"
# Build path that has GGML_VULKAN=ON; the CPU-only build under $LOCAL_BIN
# does not include libggml-vulkan.so.
GPU_BUILD="${GPU_BUILD:-$SCRIPT_DIR/../references/llama.cpp/build-release/bin}"

start_gpu_embedding() {
    if [ -f "$GPU_EMBEDDING_PID_FILE" ] && kill -0 "$(cat "$GPU_EMBEDDING_PID_FILE")" 2>/dev/null; then
        echo "[Embedding-GPU] Already running (PID $(cat "$GPU_EMBEDDING_PID_FILE"))"
        return 0
    fi
    if [ ! -x "$GPU_BUILD/llama-server" ] || [ ! -f "$GPU_BUILD/libggml-vulkan.so.0.26.0" ]; then
        echo "[Embedding-GPU] skip: Vulkan-enabled build not at $GPU_BUILD" >&2
        echo "                run: cmake -B $SCRIPT_DIR/../references/llama.cpp/build-release \\" >&2
        echo "                         -DGGML_VULKAN=ON ..." >&2
        return 1
    fi
    if [ ! -f "$EMBEDDING_MODEL" ]; then
        echo "[Embedding-GPU] Model not found: $EMBEDDING_MODEL" >&2
        return 1
    fi
    echo "[Embedding-GPU] Starting (vulkan, port $GPU_EMBEDDING_PORT)..."
    # No CPU affinity on purpose: this server's compute runs on the GPU, the
    # CPU threads only orchestrate. 2 threads is plenty for a 1-chunk-at-a-
    # time search workload.
    LD_LIBRARY_PATH="$GPU_BUILD" \
        nohup "$GPU_BUILD/llama-server" \
            -m "$EMBEDDING_MODEL" \
            --host 0.0.0.0 --port "$GPU_EMBEDDING_PORT" \
            --threads 2 -c 1024 --parallel 4 \
            --pooling "$EMBEDDING_POOLING" --embeddings \
            -ngl 99 \
            > "$GPU_EMBEDDING_LOG" 2>&1 < /dev/null &
    local pid=$!
    echo "$pid" > "$GPU_EMBEDDING_PID_FILE"
    local waited=0
    while [ "$waited" -lt 60 ]; do
        if ! kill -0 "$pid" 2>/dev/null; then
            echo "[Embedding-GPU] Failed to start (exited during load). Check $GPU_EMBEDDING_LOG"
            rm -f "$GPU_EMBEDDING_PID_FILE"
            return 1
        fi
        if curl -sf -m 2 -o /dev/null "http://127.0.0.1:$GPU_EMBEDDING_PORT/health" 2>/dev/null; then
            echo "[Embedding-GPU] Started (PID $pid, port $GPU_EMBEDDING_PORT)"
            return 0
        fi
        sleep 1
        waited=$((waited + 1))
    done
    echo "[Embedding-GPU] Timed out after ${waited}s. Check $GPU_EMBEDDING_LOG"
    rm -f "$GPU_EMBEDDING_PID_FILE"
    return 1
}

stop_gpu_embedding() {
    if [ ! -f "$GPU_EMBEDDING_PID_FILE" ]; then
        echo "[Embedding-GPU] Not running"
        return 0
    fi
    local pid
    pid=$(cat "$GPU_EMBEDDING_PID_FILE")
    if kill -0 "$pid" 2>/dev/null; then
        echo "[Embedding-GPU] Stopping (PID $pid)..."
        kill -TERM "-$pid" 2> /dev/null || kill "$pid" 2> /dev/null || true
        local waited=0
        while kill -0 "$pid" 2>/dev/null && [ "$waited" -lt 15 ]; do
            sleep 1
            waited=$((waited + 1))
        done
        if kill -0 "$pid" 2>/dev/null; then
            kill -9 "-$pid" 2> /dev/null || kill -9 "$pid" 2> /dev/null || true
        fi
        echo "[Embedding-GPU] Stopped"
    else
        echo "[Embedding-GPU] Process not found, cleaning pid file"
    fi
    rm -f "$GPU_EMBEDDING_PID_FILE"
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
        # Optional second embedding server on the GPU, used by the backend's
        # search path. Starts only if the Vulkan-enabled build is present; on
        # cloud-only or CPU-only boxes this is a no-op.
        start_gpu_embedding || true
        # Optional fast-path LLM. Brings up a small (350M-class) model on
        # $LLM_FAST_PORT for per-turn `use_fast` requests. Skipped when
        # the gguf is missing or LLM_FAST_ENABLED=false so a fresh
        # checkout still works with just the 24B.
        if [ "$LLM_FAST_ENABLED" = "true" ] && [ -n "$LLM_FAST_MODEL" ]; then
            start_service "LLM-Fast" "$LLM_FAST_PID_FILE" "$LLM_FAST_LOG" \
                "${LLM_FAST_BACKEND:-auto}" "$LLM_FAST_MODEL" "$LLM_FAST_PORT" chat-fast || true
        fi

        echo ""
        echo "=== Services ==="
        echo "  Embedding (indexer): http://127.0.0.1:$EMBEDDING_PORT/v1/embeddings"
        echo "  Embedding (search):  http://127.0.0.1:$GPU_EMBEDDING_PORT/v1/embeddings"
        echo "  Chat API:            http://127.0.0.1:$LLM_PORT/v1/chat/completions"
        echo "  Models API:          http://127.0.0.1:$LLM_PORT/v1/models"
        if [ "$LLM_FAST_ENABLED" = "true" ] && [ -n "$LLM_FAST_MODEL" ] && [ -f "$LLM_FAST_PID_FILE" ] && kill -0 "$(cat "$LLM_FAST_PID_FILE" 2>/dev/null)" 2>/dev/null; then
            echo "  Chat API (fast):     http://127.0.0.1:$LLM_FAST_PORT/v1/chat/completions"
        fi
        echo ""
        echo "Logs: $LOG_DIR/"
        ;;
    start-gpu-embed)
        # Bring up just the GPU embedding server without touching the rest.
        # Useful when the LITE embedding is already running and you only
        # want to add the search-dedicated path.
        show_backends
        echo ""
        start_gpu_embedding
        ;;
    start-fast)
        # Bring up the fast-path LLM in isolation. Useful when the main
        # stack is already running and the user just dropped the 350M
        # gguf into the model dir.
        if [ -z "$LLM_FAST_MODEL" ]; then
            echo "error: no fast model under $SCRIPT_DIR/models/lfm25-350m-q4_0/." >&2
            exit 1
        fi
        LLM_FAST_ENABLED=true start_service "LLM-Fast" "$LLM_FAST_PID_FILE" "$LLM_FAST_LOG" \
            "${LLM_FAST_BACKEND:-auto}" "$LLM_FAST_MODEL" "$LLM_FAST_PORT" chat-fast
        ;;
    stop-fast)
        stop_service "LLM-Fast" "$LLM_FAST_PID_FILE"
        ;;
    stop)
        echo "=== Stopping LLM Services ==="
        stop_service "LLM" "$LLM_PID_FILE"
        stop_service "Embedding" "$EMBEDDING_PID_FILE"
        stop_service "LLM-Fast" "$LLM_FAST_PID_FILE"
        stop_gpu_embedding
        ;;
    stop-gpu-embed)
        stop_gpu_embedding
        ;;
    status)
        echo "=== LLM Services Status ==="
        status_service "Embedding" "$EMBEDDING_PID_FILE" "$EMBEDDING_PORT"
        status_service "LLM" "$LLM_PID_FILE" "$LLM_PORT"
        if [ "$LLM_FAST_ENABLED" = "true" ] || [ -f "$LLM_FAST_PID_FILE" ]; then
            status_service "LLM-Fast" "$LLM_FAST_PID_FILE" "$LLM_FAST_PORT"
        fi
        echo ""
        show_backends
        ;;
    restart)
        "$0" stop
        sleep 2
        "$0" start
        ;;
    restart-fast)
        "$0" stop-fast || true
        sleep 1
        "$0" start-fast
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
        echo "  LLM_THREADS=8        LLM inference threads         (override)"
        echo "  EMB_THREADS=3        embedding inference threads   (override)"
        echo "  LLM_CPUSET=0xFC3     pin LLM to these CPUs (hex)   (auto-detected on big.LITTLE)"
        echo "  EMB_CPUSET=0x3C      pin embedding to these CPUs   (auto-detected on big.LITTLE)"
        echo ""
        echo "Logs: $LOG_DIR/"
        ;;
    *)
        echo "Usage: $0 {install|start|stop|status|restart|help}"
        echo "Run '$0 help' for details."
        exit 1
        ;;
esac