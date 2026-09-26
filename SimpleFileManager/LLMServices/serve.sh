#!/bin/bash
#
# LLM Services - Start/Stop embedding and chat services
#
# Usage:
#   ./serve.sh start    - Start both services in background
#   ./serve.sh stop     - Stop both services
#   ./serve.sh status   - Check if services are running
#   ./serve.sh restart  - Restart both services
#

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BIN="$SCRIPT_DIR/bin/rust-model-inference"
LOG_DIR="$SCRIPT_DIR/logs"
PID_DIR="$SCRIPT_DIR/logs"

EMBEDDING_MODEL="$SCRIPT_DIR/models/embedding/Qwen3-Embedding-0.6B-Q8_0.gguf"
EMBEDDING_PORT=9011
EMBEDDING_PID_FILE="$PID_DIR/embedding.pid"
EMBEDDING_LOG="$LOG_DIR/embedding.log"

LLM_MODEL="$SCRIPT_DIR/models/llm/LFM2.5-8B-A1B-Q8_0.gguf"
LLM_PORT=9010
LLM_PID_FILE="$PID_DIR/llm.pid"
LLM_LOG="$LOG_DIR/llm.log"

THREADS=8

mkdir -p "$LOG_DIR"

start_service() {
    local name="$1"
    local pid_file="$2"
    local log_file="$3"
    shift 3

    if [ -f "$pid_file" ] && kill -0 "$(cat "$pid_file")" 2>/dev/null; then
        echo "[$name] Already running (PID $(cat "$pid_file"))"
        return 0
    fi

    echo "[$name] Starting..."
    nohup "$BIN" "$@" > "$log_file" 2>&1 &
    local pid=$!
    echo "$pid" > "$pid_file"
    sleep 3

    if kill -0 "$pid" 2>/dev/null; then
        echo "[$name] Started (PID $pid, log: $log_file)"
    else
        echo "[$name] Failed to start! Check $log_file"
        rm -f "$pid_file"
        return 1
    fi
}

stop_service() {
    local name="$1"
    local pid_file="$2"

    if [ ! -f "$pid_file" ]; then
        echo "[$name] Not running"
        return 0
    fi

    local pid=$(cat "$pid_file")
    if kill -0 "$pid" 2>/dev/null; then
        echo "[$name] Stopping (PID $pid)..."
        kill "$pid"
        sleep 2
        if kill -0 "$pid" 2>/dev/null; then
            echo "[$name] Force killing..."
            kill -9 "$pid"
        fi
        echo "[$name] Stopped"
    else
        echo "[$name] Process not found, cleaning pid file"
    fi
    rm -f "$pid_file"
}

status_service() {
    local name="$1"
    local pid_file="$2"
    local port="$3"

    if [ -f "$pid_file" ] && kill -0 "$(cat "$pid_file")" 2>/dev/null; then
        echo "[$name] Running (PID $(cat "$pid_file"), port $port)"
    else
        echo "[$name] Not running"
    fi
}

case "${1:-}" in
    start)
        echo "=== Starting LLM Services ==="
        start_service "Embedding" "$EMBEDDING_PID_FILE" "$EMBEDDING_LOG" \
            --serve --model "$EMBEDDING_MODEL" --embedding \
            --host 0.0.0.0 --port "$EMBEDDING_PORT" --threads "$THREADS"

        start_service "LLM" "$LLM_PID_FILE" "$LLM_LOG" \
            --serve --model "$LLM_MODEL" \
            --host 0.0.0.0 --port "$LLM_PORT" --threads "$THREADS"

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
        ;;

    restart)
        "$0" stop
        sleep 2
        "$0" start
        ;;

    help|--help|-h)
        echo "LLM Services - Manage embedding and chat API services"
        echo ""
        echo "Usage: $0 {start|stop|status|restart|help}"
        echo ""
        echo "Commands:"
        echo "  start    Start embedding + chat services in background"
        echo "  stop     Stop both services"
        echo "  status   Show running status"
        echo "  restart  Restart both services"
        echo "  help     Show this help message"
        echo ""
        echo "Services:"
        echo "  Chat       http://127.0.0.1:$LLM_PORT/v1/chat/completions"
        echo "  Embedding  http://127.0.0.1:$EMBEDDING_PORT/v1/embeddings"
        echo "  Models     http://127.0.0.1:$LLM_PORT/v1/models"
        echo ""
        echo "Logs: $LOG_DIR/"
        ;;

    *)
        echo "Usage: $0 {start|stop|status|restart|help}"
        echo "Run '$0 help' for details."
        exit 1
        ;;
esac
