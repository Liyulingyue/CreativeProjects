#!/bin/bash
# [v=2026-04-29T19:40:00]

set -e

MANIFEST="output/splits/ERNIE-4.5-21B-A3B-PT-k6/manifest.json"
WORKERS_DIR="output/splits/ERNIE-4.5-21B-A3B-PT-k6/experts"
LOG_DIR="logs"
MASTER_LOG="$LOG_DIR/master.log"
WORKER_LOG="$LOG_DIR/worker.log"
DTYPE="float32"
KV_CACHE="true"

echo "[v=2026-04-29T19:40:00] Step 1: Kill old processes and remove logs"
ps aux | grep "nexus" | grep -v grep | awk '{print $2}' | xargs kill -9 2>/dev/null || true
sleep 2
rm -rf "$LOG_DIR"
mkdir -p "$LOG_DIR"

echo "[v=2026-04-29T19:40:00] Step 2: Start Master (experts 0,1,2)"
# 若KV_CACHE=true，则master会在内存中缓存专家的key/value，worker每次调用专家时会先从master获取key/value，如果缓存命中则直接返回结果，否则才会调用专家计算并返回结果。这样可以减少worker与master之间的通信开销，提高性能。
if [ "$KV_CACHE" = "true" ]; then
    echo "  Using KV cache: master will cache expert key/value in memory, worker will first check cache before calling experts."
    .venv/bin/python src/nexus/master.py \
        --manifest "$MANIFEST" \
        --port 5000 \
        --host 127.0.0.1 \
        --dtype "$DTYPE" \
        --experts 0,1,2 \
        --log-file "$MASTER_LOG" \
        --kv-cache &
else
    echo "  Not using KV cache: worker will call experts directly without caching key/value in
    memory. This may increase communication overhead between worker and master, but can save memory if the model is large."
    .venv/bin/python src/nexus/master.py \
        --manifest "$MANIFEST" \
        --port 5000 \
        --host 127.0.0.1 \
        --dtype "$DTYPE" \
        --experts 0,1,2 \
        --log-file "$MASTER_LOG" &
fi
MASTER_PID=$!
echo "  Master PID=$MASTER_PID"

echo "[v=2026-04-29T19:40:00] Step 3: Wait for Master to be ready (up to 120s)"
for i in $(seq 1 120); do
    if grep -q "Nexus Master ready" "$MASTER_LOG" 2>/dev/null; then
        echo "  Master ready!"
        break
    fi
    sleep 1
done

echo "[v=2026-04-29T19:40:00] Step 4: Start Worker (experts 3,4,5)"
.venv/bin/python src/nexus/worker.py \
    --id worker-1 \
    --http-port 8000 \
    --tcp-port 9000 \
    --host 127.0.0.1 \
    --dtype "$DTYPE" \
    --experts-dir "$WORKERS_DIR" \
    --expert-ids 3,4,5 \
    --master http://127.0.0.1:5000 \
    --log-file "$WORKER_LOG" &
WORKER_PID=$!
echo "  Worker PID=$WORKER_PID"

echo "[v=2026-04-29T19:40:00] Step 5: Wait for Worker to register (up to 30s)"
for i in $(seq 1 30); do
    if grep -q "Worker registered" "$MASTER_LOG" 2>/dev/null; then
        echo "  Worker registered!"
        break
    fi
    sleep 1
done

echo "[v=2026-04-29T19:40:00] Step 6: Run inference"
sleep 2
REQUEST_BODY='{"prompt":"What a nice day!","max_tokens":20}'
response=$(curl -sS -X POST "http://127.0.0.1:5000/inference" \
    -H "Content-Type: application/json; charset=utf-8" \
    -H "Accept: application/json" \
    --data-binary "$REQUEST_BODY" \
    --max-time 600 -w "\n%{http_code}")
http_code=$(printf '%s' "$response" | tail -n1)
body=$(printf '%s' "$response" | sed '$d')
if [ "$http_code" -ne 200 ]; then
    echo "  FAIL: status=$http_code body=$body"
    echo "[v=2026-04-29T19:40:00] Cleanup: kill master($MASTER_PID) worker($WORKER_PID)"
    kill $MASTER_PID $WORKER_PID 2>/dev/null || true
    exit 1
fi

echo "  OK: status=$http_code body=$body"

echo "[v=2026-04-29T19:40:00] Cleanup: kill master($MASTER_PID) worker($WORKER_PID)"
kill $MASTER_PID $WORKER_PID 2>/dev/null || true
