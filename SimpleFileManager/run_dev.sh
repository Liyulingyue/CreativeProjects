#!/bin/bash
#
# run_dev.sh - SimpleFileManager development stack manager
#
# Usage:
#   ./run_dev.sh --help
#   ./run_dev.sh --start all        # LLM services + backend + frontend
#   ./run_dev.sh --start backend
#   ./run_dev.sh --start frontend
#   ./run_dev.sh --start llm
#   ./run_dev.sh --stop [component] # default: all except llm
#   ./run_dev.sh --status
#   ./run_dev.sh --restart [component]
#
# Ports (override via environment):
#   FRONTEND_PORT   Vite dev server      (default: read from vite.config.ts)
#   BACKEND_PORT    FastAPI              (default 8000)
#
# LLM services (LLMServices/serve.sh) on this box pair LFM2.5-24B-A2B Q4_K_M
# on the LLM side with bge-base-zh-v1.5 Q8_0 (105 MB, 768-dim) on the
# embedding side. Both stay on CPU; llama-server is CPU-pinned to the A720
# big cores for chat and to the A520 little cores for the embedding slice so
# the two workloads don't fight for shared cache lines. We deliberately do
# not move embedding to the big cores even though it would shave hours off
# the indexing ETA: the chat workload's tail latency is the product
# constraint, and big.LITTLE lets both keep their respective SLO.
# Override per service with LLM_THREADS, EMB_THREADS, LLM_CPUSET,
# EMB_CPUSET environment variables before --start llm. Embedding model
# choice and dim are picked up from .env (EMBEDDING_MODEL / EMBEDDING_DIM).
#
# Logs: logs/backend.log (rotated), logs/backend.console.log, logs/frontend.log
# Pids: logs/{backend,frontend,llm}.pid
#
set -u

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BACKEND_DIR="$ROOT/backend"
FRONTEND_DIR="$ROOT/frontend"
LLM_DIR="$ROOT/LLMServices"
LOG_DIR="$ROOT/logs"

BACKEND_PORT="${BACKEND_PORT:-8000}"

# Vite's port may be pinned in vite.config.ts rather than taken from the
# environment, so read it back instead of assuming the 5173 default.
if [ -z "${FRONTEND_PORT:-}" ] && [ -f "$FRONTEND_DIR/vite.config.ts" ]; then
    FRONTEND_PORT="$(grep -oE 'port:[[:space:]]*[0-9]+' "$FRONTEND_DIR/vite.config.ts" \
                    | head -n 1 | grep -oE '[0-9]+' || true)"
fi
FRONTEND_PORT="${FRONTEND_PORT:-5173}"

mkdir -p "$LOG_DIR"

BACKEND_PID="$LOG_DIR/backend.pid"
FRONTEND_PID="$LOG_DIR/frontend.pid"
WATCHDOG_PID="$LOG_DIR/watchdog.pid"

c_green() { printf '\033[32m%s\033[0m\n' "$1"; }
c_red()   { printf '\033[31m%s\033[0m\n' "$1"; }
c_dim()   { printf '\033[2m%s\033[0m\n' "$1"; }

# --- process helpers -------------------------------------------------------

port_busy() {
    (exec 3<>"/dev/tcp/127.0.0.1/$1") 2>/dev/null && { exec 3<&- 2>/dev/null; return 0; } || return 1
}

pid_alive() {
    local file="$1"
    [ -f "$file" ] || return 1
    local pid
    pid="$(cat "$file" 2>/dev/null)"
    [ -n "$pid" ] && kill -0 "$pid" 2>/dev/null
}

# Wait for a port, but give up early if the process we launched has died.
wait_for_port() {
    local port="$1" pid_file="${2:-}" tries="${3:-40}" i=0
    while [ "$i" -lt "$tries" ]; do
        port_busy "$port" && return 0
        if [ -n "$pid_file" ] && [ -f "$pid_file" ]; then
            local pid
            pid="$(cat "$pid_file" 2>/dev/null)"
            if [ -n "$pid" ] && ! kill -0 "$pid" 2>/dev/null; then
                return 1
            fi
        fi
        sleep 1
        i=$((i + 1))
    done
    return 1
}

# Signal a pid's whole process group first: npm/vite and uvicorn --reload both
# fork children, and a plain kill to the parent orphans them.
terminate() {
    local name="$1" pid_file="$2"
    if [ ! -f "$pid_file" ]; then
        c_dim "  $name: not running"
        return 0
    fi
    local pid
    pid="$(cat "$pid_file" 2>/dev/null)"
    if [ -z "$pid" ] || ! kill -0 "$pid" 2>/dev/null; then
        c_dim "  $name: stale pid file, cleaning"
        rm -f "$pid_file"
        return 0
    fi
    kill -TERM "-$pid" 2>/dev/null || kill -TERM "$pid" 2>/dev/null
    local waited=0
    while kill -0 "$pid" 2>/dev/null && [ "$waited" -lt 10 ]; do
        sleep 1
        waited=$((waited + 1))
    done
    if kill -0 "$pid" 2>/dev/null; then
        kill -9 "-$pid" 2>/dev/null || kill -9 "$pid" 2>/dev/null
        c_red "  $name: force killed (PID $pid)"
    else
        c_green "  $name: stopped (PID $pid)"
    fi
    rm -f "$pid_file"
}

fail_with_log() {
    local name="$1" log="$2"
    c_red "  $name failed to start. Last 20 lines of $(basename "$log"):"
    tail -n 20 "$log" 2>/dev/null | sed 's/^/    /'
    rm -f "$3" 2>/dev/null
    return 1
}

# The pids listening on a port, if we can tell.
port_holders() {
    local port="$1"
    ss -ltnp 2>/dev/null | awk -v p=":$port\$" '
        $4 ~ p {
            line = $0
            while (match(line, /pid=[0-9]+/)) {
                print substr(line, RSTART + 4, RLENGTH - 4)
                line = substr(line, RSTART + RLENGTH)
            }
        }' | sort -u
}

# Is this process one of ours? A leftover from an earlier run of this
# script is ours to clean up; somebody else's process is not.
is_our_process() {
    local pid="$1" dir="$2"
    local cwd
    cwd="$(readlink "/proc/$pid/cwd" 2>/dev/null)"
    case "$cwd" in
        "$dir"|"$dir"/*) return 0 ;;
    esac
    local cmd
    cmd="$(tr '\0' ' ' < "/proc/$pid/cmdline" 2>/dev/null)"
    case "$cmd" in
        *"$dir"*) return 0 ;;
    esac
    return 1
}

# Free a port that is held by a leftover of ours.
#
# With hot reload on, the backend is a supervisor and a worker. Kill the
# supervisor -- which is what the OOM killer does when it picks a
# victim -- and the worker is orphaned but keeps the port. The service
# still answers, so the watchdog sees nothing wrong; but the next
# restart says "the port is in use" and never comes back.
#
# So: when the port is busy and the pid we recorded is gone, look at
# who holds it. If it is one of ours, stop it and carry on. If it is
# somebody else's, leave it alone and say so.
reclaim_port() {
    local port="$1" dir="$2" name="$3"
    local holders pid mine=""
    holders="$(port_holders "$port")"
    [ -n "$holders" ] || return 1
    for pid in $holders; do
        if is_our_process "$pid" "$dir"; then
            mine="$mine $pid"
        else
            return 1
        fi
    done
    [ -n "$mine" ] || return 1
    c_dim "  $name: a leftover of ours (PID$(echo $mine)) is holding port $port; stopping it"
    for pid in $mine; do
        kill -TERM "-$pid" 2>/dev/null || kill -TERM "$pid" 2>/dev/null
    done
    local waited=0
    while port_busy "$port" && [ "$waited" -lt 10 ]; do
        sleep 1
        waited=$((waited + 1))
    done
    if port_busy "$port"; then
        for pid in $mine; do
            kill -9 "-$pid" 2>/dev/null || kill -9 "$pid" 2>/dev/null
        done
        sleep 1
    fi
    ! port_busy "$port"
}

# --- components ------------------------------------------------------------

start_llm() {
    echo "[llm] LLM services..."
    if [ ! -x "$LLM_DIR/serve.sh" ]; then
        c_red "  LLMServices/serve.sh not found or not executable"
        return 1
    fi
    "$LLM_DIR/serve.sh" start
}

stop_llm() {
    echo "[llm] LLM services..."
    if [ -x "$LLM_DIR/serve.sh" ]; then
        "$LLM_DIR/serve.sh" stop
    fi
}

start_backend() {
    echo "[backend] FastAPI..."
    if pid_alive "$BACKEND_PID"; then
        c_dim "  already running (PID $(cat "$BACKEND_PID"))"
        return 0
    fi
    if port_busy "$BACKEND_PORT"; then
        # A leftover of ours may still be holding the port (see
        # reclaim_port). If so, take it back; if not, it is somebody
        # else's and we say so.
        if ! reclaim_port "$BACKEND_PORT" "$BACKEND_DIR" "backend"; then
            c_red "  port $BACKEND_PORT is in use by another process."
            c_red "  Stop it first, or set BACKEND_PORT to a free port."
            return 1
        fi
    fi
    if [ ! -d "$BACKEND_DIR/.venv" ]; then
        c_red "  backend/.venv missing. Create it:"
        c_red "    cd backend && python -m venv .venv && .venv/bin/pip install -r requirements.txt"
        return 1
    fi
    # Boot needs INIT_PASSWORD or ALLOW_INSECURE_NO_AUTH in $ROOT/.env, or it
    # will refuse to start in the lifespan handler. A pristine checkout only
    # ships .env.example, so seed .env once and warn loudly when neither auth
    # switch is set.
    if [ ! -f "$ROOT/.env" ]; then
        if [ -f "$ROOT/.env.example" ]; then
            cp "$ROOT/.env.example" "$ROOT/.env"
            c_dim "  seeded $ROOT/.env from .env.example -- edit INIT_PASSWORD before any real use"
        else
            c_red "  no $ROOT/.env and no .env.example to copy from"
            return 1
        fi
    fi
    if ! grep -qE '^[[:space:]]*INIT_PASSWORD=' "$ROOT/.env" \
       && ! grep -qE '^[[:space:]]*ALLOW_INSECURE_NO_AUTH=' "$ROOT/.env"; then
        c_red "  $ROOT/.env is missing both INIT_PASSWORD and ALLOW_INSECURE_NO_AUTH."
        c_red "  Backend will refuse to start. Edit .env and add one of them."
        return 1
    fi
    # .env lives at the repo root; deps.py resolves STORAGE_PATH relative to
    # backend/, so ../data keeps dev and docker on the same files.
    # Reload off: it spawns a supervisor+worker, so the recorded pid would
    # not be the process holding the port.
    #
    # The app writes its own rotated log to backend.log, and also to
    # stdout for docker. stdout is dropped here so the two do not
    # duplicate each other; stderr still lands in a console file,
    # which catches whatever bypassed the logging system.
    #
    # setsid puts the service in a session of its own, so closing the
    # terminal that started it does not take it down with it. A NAS
    # that stops when the shell that launched it goes is not a NAS.
    # Hot reload is on. Without it, a change to the backend needs a
    # manual restart, and the first sign that the change did not take
    # effect is a stale answer -- which reads as "the fix did not work"
    # rather than "the server is running the old code".
    #
    # The reload spawns a supervisor and a worker, so the pid file
    # holds the supervisor. Stopping sends TERM to the whole process
    # group, which takes both down; that is what terminate() does.
    setsid bash -c '
        cd "$1" || exit 1
        LOG_FILE="$2" exec .venv/bin/python run.py
    ' _ "$BACKEND_DIR" "$LOG_DIR/backend.log" > /dev/null 2> "$LOG_DIR/backend.console.log" &
    echo $! > "$BACKEND_PID"
    disown 2>/dev/null || true
    if wait_for_port "$BACKEND_PORT" "$BACKEND_PID" 40; then
        c_green "  up on :$BACKEND_PORT (PID $(cat "$BACKEND_PID"))"
        return 0
    fi
    fail_with_log "backend" "$LOG_DIR/backend.console.log" "$BACKEND_PID"
}

start_frontend() {
    echo "[frontend] Vite dev server..."
    if pid_alive "$FRONTEND_PID"; then
        c_dim "  already running (PID $(cat "$FRONTEND_PID"))"
        return 0
    fi
    if port_busy "$FRONTEND_PORT"; then
        if ! reclaim_port "$FRONTEND_PORT" "$FRONTEND_DIR" "frontend"; then
            c_red "  port $FRONTEND_PORT is in use by another process."
            c_red "  Stop it first, or edit port in frontend/vite.config.ts."
            return 1
        fi
    fi
    # Invoke vite directly rather than `npm run dev`: npm reparents, leaving the
    # pid file pointing at a dead shell and breaking stop/status.
    if [ ! -x "$FRONTEND_DIR/node_modules/.bin/vite" ]; then
        c_red "  frontend/node_modules missing or vite not installed."
        c_red "  Run: cd frontend && npm install"
        return 1
    fi
    setsid bash -c '
        cd "$1" || exit 1
        exec ./node_modules/.bin/vite
    ' _ "$FRONTEND_DIR" > "$LOG_DIR/frontend.log" 2>&1 &
    echo $! > "$FRONTEND_PID"
    disown 2>/dev/null || true
    if wait_for_port "$FRONTEND_PORT" "$FRONTEND_PID" 40; then
        c_green "  up on :$FRONTEND_PORT (PID $(cat "$FRONTEND_PID"))"
        return 0
    fi
    fail_with_log "frontend" "$LOG_DIR/frontend.log" "$FRONTEND_PID"
}

stop_backend()  { echo "[backend]"; terminate backend "$BACKEND_PID"; }
stop_frontend() { echo "[frontend]"; terminate frontend "$FRONTEND_PID"; }
stop_watchdog() { echo "[watchdog]"; terminate watchdog "$WATCHDOG_PID"; }

# Something has to notice when a service dies.
#
# This machine has 30GB of memory and the 24B model alone takes 18GB of
# it. When the kernel runs out, it takes something with it -- and one
# afternoon it took the backend, the model and the picture index
# server all at once. Nothing restarted them: the developer had to
# notice. A watchdog that checks every half minute is cheaper than
# that.
start_watchdog() {
    if pid_alive "$WATCHDOG_PID"; then
        c_dim "  already running (PID $(cat "$WATCHDOG_PID"))"
        return 0
    fi
    setsid bash -c '
        ROOT="$1"; LOG="$2"; PORT="$3"; PID_FILE="$4"
        echo "[watchdog] starting" > "$LOG"
        last_llm_retry=0
        while true; do
            sleep 30
            # The backend: two things have to be true. It has to answer,
            # and the process we started has to still be there -- with
            # hot reload on, that process is a supervisor, and if it is
            # gone the worker it left behind keeps answering while
            # code changes stop taking effect. A service that answers
            # but does not reload is broken in a way nobody notices.
            bpid=""
            [ -f "$PID_FILE" ] && bpid="$(cat "$PID_FILE" 2>/dev/null)"
            supervisor_alive=0
            [ -n "$bpid" ] && kill -0 "$bpid" 2>/dev/null && supervisor_alive=1
            if ! curl -sf -m 5 "http://127.0.0.1:$PORT/api/health/live" >/dev/null 2>&1 \
               || [ "$supervisor_alive" -ne 1 ]; then
                echo "[watchdog] $(date +%T) the backend is not right (answering=$([ "$supervisor_alive" -eq 1 ] && echo yes || echo no)); starting it again" >> "$LOG"
                "$ROOT/run_dev.sh" --start backend >> "$LOG" 2>&1
            fi
            # The model: if it is gone, it is worth starting again --
            # but not every thirty seconds, the model takes a while to
            # come up.
            if ! curl -sf -m 5 "http://127.0.0.1:9010/health" >/dev/null 2>&1; then
                now=$(date +%s)
                if [ $((now - last_llm_retry)) -gt 120 ]; then
                    last_llm_retry=$now
                    echo "[watchdog] $(date +%T) the model is not answering; starting it again" >> "$LOG"
                    "$ROOT/LLMServices/serve.sh" start >> "$LOG" 2>&1
                fi
            fi
        done
    ' _ "$ROOT" "$LOG_DIR/watchdog.log" "$BACKEND_PORT" "$BACKEND_PID" > /dev/null 2>&1 &
    echo $! > "$WATCHDOG_PID"
    disown 2>/dev/null || true
    c_green "  watching over the services (PID $(cat "$WATCHDOG_PID"))"
}

status_one() {
    local name="$1" pid_file="$2" port="$3"
    if pid_alive "$pid_file"; then
        c_green "  $name: running (PID $(cat "$pid_file"), port $port)"
    elif port_busy "$port"; then
        c_dim "  $name: port $port in use by another process (not started here)"
    else
        c_red "  $name: stopped"
    fi
}

status_llm() {
    if [ -x "$LLM_DIR/serve.sh" ]; then
        "$LLM_DIR/serve.sh" status
    fi
}

# --- argument parsing ------------------------------------------------------

resolve_targets() {
    # $1 = comma list or single word; empty means "all"
    case "${1:-all}" in
        all)      echo "llm backend frontend" ;;
        llm)      echo "llm" ;;
        backend)  echo "backend" ;;
        frontend) echo "frontend" ;;
        *)        echo "" ;;
    esac
}

usage() {
    cat <<EOF
run_dev.sh - SimpleFileManager development stack manager

Usage:
  ./run_dev.sh --help
  ./run_dev.sh --start [TARGET]
  ./run_dev.sh --stop  [TARGET]
  ./run_dev.sh --status
  ./run_dev.sh --restart [TARGET]

TARGET:
  all         LLM services + backend + frontend  (default)
  llm         local inference services only
  backend     FastAPI only
  frontend    Vite dev server only

Commands:
  --start [TARGET]     Start the given target(s)
  --stop  [TARGET]     Stop the given target(s)
  --status             Show what is running, including LLM services
  --restart [TARGET]   Stop then start

Examples:
  ./run_dev.sh --start all
  ./run_dev.sh --start backend
  ./run_dev.sh --stop frontend
  ./run_dev.sh --restart all

Notes:
  - "llm" is never stopped implicitly by --stop all, because the inference
    services may be shared. Stop them explicitly with --stop llm.
  - The frontend port is read from frontend/vite.config.ts; override with
    FRONTEND_PORT=<n>. Backend port: BACKEND_PORT=<n> (default 8000).
  - run.py honours BACKEND_PORT and BACKEND_RELOAD=0 disables hot reload.

Logs: $LOG_DIR/backend.log (rotated), $LOG_DIR/{backend.console,frontend}.log
Pids:  $LOG_DIR/{backend,frontend}.pid
EOF
}

banner() {
    c_dim "  frontend :$FRONTEND_PORT    backend :$BACKEND_PORT    logs: $LOG_DIR"
    echo ""
}

ACTION=""
TARGET=""

while [ $# -gt 0 ]; do
    case "$1" in
        --help|-h)   usage; exit 0 ;;
        --start)     ACTION="start";   shift; TARGET="${1:-all}"; shift || true ;;
        --stop)      ACTION="stop";    shift; TARGET="${1:-all}"; shift || true ;;
        --restart)   ACTION="restart"; shift; TARGET="${1:-all}"; shift || true ;;
        --status)    ACTION="status";  shift ;;
        *)
            echo "Unknown argument: $1"
            echo "Run './run_dev.sh --help'"
            exit 1
            ;;
    esac
done

[ -z "$ACTION" ] && { usage; exit 1; }

TARGETS="$(resolve_targets "$TARGET")"
if [ -z "$TARGETS" ]; then
    c_red "Unknown target: $TARGET"
    echo "Valid targets: all, llm, backend, frontend"
    exit 1
fi

rc=0

case "$ACTION" in
    start)
        echo "=== Starting: $TARGETS ==="
        banner
        for t in $TARGETS; do
            case "$t" in
                llm)      start_llm      || rc=1 ;;
                backend)  start_backend  || rc=1 ;;
                frontend) start_frontend || rc=1 ;;
            esac
            [ "$rc" -ne 0 ] && break
        done
        # The watchdog goes up with the backend: it is what puts the
        # backend back if it dies.
        if [ "$rc" -eq 0 ] && { [ "$TARGET" = "all" ] || [ "$TARGET" = "backend" ]; }; then
            echo "[watchdog]"
            start_watchdog
        fi
        if [ "$rc" -eq 0 ]; then
            echo ""
            c_green "=== Ready ==="
            if [ "$TARGET" = "all" ]; then
                echo "  App:  http://localhost:$FRONTEND_PORT"
                echo "  API:  http://localhost:$BACKEND_PORT/api"
            fi
            echo "  Stop: ./run_dev.sh --stop all"
        else
            echo ""
            c_red "=== Start failed ==="
        fi
        ;;

    stop)
        # --stop all intentionally leaves llm alone; it may be shared.
        STOP_TARGETS="$TARGETS"
        if [ "$TARGET" = "all" ]; then
            STOP_TARGETS="backend frontend"
        fi
        echo "=== Stopping: $STOP_TARGETS ==="
        for t in $STOP_TARGETS; do
            case "$t" in
                llm)      stop_llm      ;;
                backend)  stop_backend  ;;
                frontend) stop_frontend ;;
            esac
        done
        if [ "$TARGET" = "all" ]; then
            # The watchdog would otherwise put everything back up
            # again a few seconds after it was stopped.
            stop_watchdog
            echo ""
            c_dim "  LLM services left running. Stop them with: ./run_dev.sh --stop llm"
        fi
        ;;

    restart)
        echo "=== Restarting: $TARGETS ==="
        STOP_TARGETS="$TARGETS"
        if [ "$TARGET" = "all" ]; then
            STOP_TARGETS="backend frontend"
        fi
        for t in $STOP_TARGETS; do
            case "$t" in
                llm)      stop_llm      ;;
                backend)  stop_backend  ;;
                frontend) stop_frontend ;;
            esac
        done
        sleep 2
        echo ""
        for t in $TARGETS; do
            case "$t" in
                llm)      start_llm      || rc=1 ;;
                backend)  start_backend  || rc=1 ;;
                frontend) start_frontend || rc=1 ;;
            esac
            [ "$rc" -ne 0 ] && break
        done
        [ "$rc" -eq 0 ] && { echo ""; c_green "=== Restarted ==="; } || { echo ""; c_red "=== Restart failed ==="; }
        ;;

    status)
        echo "=== Dev stack status ==="
        status_one backend  "$BACKEND_PID"  "$BACKEND_PORT"
        status_one frontend "$FRONTEND_PID" "$FRONTEND_PORT"
        echo ""
        status_llm
        ;;
esac

exit $rc
