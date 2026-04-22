#!/bin/bash
set -e
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

echo "Killing existing processes..."
sudo pkill -f raspbot.py || true
pkill -f "camera_udp.py" || true
sleep 1

if [ ! -d ".venv" ]; then
    echo "Creating virtual environment..."
    python3 -m venv .venv
fi

echo "Installing dependencies..."
. .venv/bin/activate
pip install -r requirements.txt

echo "Starting Camera UDP Server..."
nohup sudo /usr/bin/python3 "$SCRIPT_DIR/app/hardware/camera_udp.py" > /tmp/camera_udp.log 2>&1 &
echo "Camera UDP PID: $!"
sleep 2

if ! ps -p $! > /dev/null; then
    echo "ERROR: camera_udp failed to start"
    cat /tmp/camera_udp.log
    exit 1
fi

echo "Camera UDP started. Log:"
tail -1 /tmp/camera_udp.log

echo "Starting FastAPI..."
python -m uvicorn app:create_app --host 0.0.0.0 --port 8000
