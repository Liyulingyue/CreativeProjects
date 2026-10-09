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
# Logs: logs/{backend,frontend}.log   Pids: logs/{backend,frontend,llm}.pid
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
        c_red "  port $BACKEND_PORT is in use by another process."
        c_red "  Stop it first, or set BACKEND_PORT to a free port."
        return 1
    fi
    if [ ! -d "$BACKEND_DIR/.venv" ]; then
        c_red "  backend/.venv missing. Create it:"
        c_red "    cd backend && python -m venv .venv && .venv/bin/pip install -r requirements.txt"
        return 1
    fi
    (
        cd "$BACKEND_DIR" || exit 1
        # .env lives at the repo root; deps.py resolves STORAGE_PATH relative to
        # backend/, so ../data keeps dev and docker on the same files.
        # Reload off: it spawns a supervisor+worker, so the recorded pid would
        # not be the process holding the port.
        BACKEND_RELOAD=0 exec .venv/bin/python run.py
    ) > "$LOG_DIR/backend.log" 2>&1 &
    echo $! > "$BACKEND_PID"
    if wait_for_port "$BACKEND_PORT" "$BACKEND_PID" 40; then
        c_green "  up on :$BACKEND_PORT (PID $(cat "$BACKEND_PID"))"
        return 0
    fi
    fail_with_log "backend" "$LOG_DIR/backend.log" "$BACKEND_PID"
}

start_frontend() {
    echo "[frontend] Vite dev server..."
    if pid_alive "$FRONTEND_PID"; then
        c_dim "  already running (PID $(cat "$FRONTEND_PID"))"
        return 0
    fi
    if port_busy "$FRONTEND_PORT"; then
        c_red "  port $FRONTEND_PORT is in use by another process."
        c_red "  Stop it first, or edit port in frontend/vite.config.ts."
        return 1
    fi
    # Invoke vite directly rather than `npm run dev`: npm reparents, leaving the
    # pid file pointing at a dead shell and breaking stop/status.
    if [ ! -x "$FRONTEND_DIR/node_modules/.bin/vite" ]; then
        c_red "  frontend/node_modules missing or vite not installed."
        c_red "  Run: cd frontend && npm install"
        return 1
    fi
    (
        cd "$FRONTEND_DIR" || exit 1
        exec ./node_modules/.bin/vite
    ) > "$LOG_DIR/frontend.log" 2>&1 &
    echo $! > "$FRONTEND_PID"
    if wait_for_port "$FRONTEND_PORT" "$FRONTEND_PID" 40; then
        c_green "  up on :$FRONTEND_PORT (PID $(cat "$FRONTEND_PID"))"
        return 0
    fi
    fail_with_log "frontend" "$LOG_DIR/frontend.log" "$FRONTEND_PID"
}

stop_backend()  { echo "[backend]"; terminate backend "$BACKEND_PID"; }
stop_frontend() { echo "[frontend]"; terminate frontend "$FRONTEND_PID"; }

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

Logs: $LOG_DIR/{backend,frontend}.log
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
