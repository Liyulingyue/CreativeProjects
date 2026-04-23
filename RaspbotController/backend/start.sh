#!/bin/bash
set -e
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

echo "Killing existing processes..."
sudo pkill -f raspbot.py || true
pkill -f "camera_udp.py" || true
pkill -f "buzzer_udp.py" || true
pkill -f "distance_udp.py" || true
pkill -f "led_udp.py" || true
pkill -f "buzzer_udp.py" || true
pkill -f "distance_udp.py" || true
pkill -f "led_udp.py" || true
pkill -f "uvicorn.*8000" || true
sleep 1

echo "Installing dependencies (if needed)..."
if [ ! -d ".venv" ]; then
    echo "Creating virtual environment..."
    python3 -m venv .venv
    . .venv/bin/activate
    pip install -r requirements.txt
elif [ "requirements.txt" -nt ".venv" ]; then
    . .venv/bin/activate
    pip install -r requirements.txt
fi

echo "Starting Camera UDP Server..."
nohup sudo /usr/bin/python3 "$SCRIPT_DIR/scripts/camera_udp.py" > /tmp/camera_udp.log 2>&1 &
echo "Camera UDP PID: $!"
sleep 2

if ! ps -p $! > /dev/null; then
    echo "ERROR: camera_udp failed to start"
    cat /tmp/camera_udp.log
    exit 1
fi

echo "Camera UDP started. Log:"
tail -1 /tmp/camera_udp.log

echo "Starting Buzzer UDP Server..."
nohup sudo /usr/bin/python3 "$SCRIPT_DIR/scripts/buzzer_udp.py" > /tmp/buzzer_udp.log 2>&1 &
echo "Buzzer UDP PID: $!"
sleep 1

if ps -p $! > /dev/null 2>&1; then
    echo "Buzzer UDP started. Log:"
    tail -1 /tmp/buzzer_udp.log
else
    echo "WARNING: buzzer_udp may have failed"
    cat /tmp/buzzer_udp.log
fi

echo "Starting Distance UDP Server..."
nohup sudo /usr/bin/python3 "$SCRIPT_DIR/scripts/distance_udp.py" > /tmp/distance_udp.log 2>&1 &
echo "Distance UDP PID: $!"
sleep 1

if ps -p $! > /dev/null 2>&1; then
    echo "Distance UDP started. Log:"
    tail -1 /tmp/distance_udp.log
else
    echo "WARNING: distance_udp may have failed"
    cat /tmp/distance_udp.log
fi

echo "Starting LED UDP Server..."
nohup sudo /usr/bin/python3 "$SCRIPT_DIR/scripts/led_udp.py" > /tmp/led_udp.log 2>&1 &
echo "LED UDP PID: $!"
sleep 1

if ps -p $! > /dev/null 2>&1; then
    echo "LED UDP started. Log:"
    tail -1 /tmp/led_udp.log
else
    echo "WARNING: led_udp may have failed"
    cat /tmp/led_udp.log
fi

echo "Starting FastAPI..."
. .venv/bin/activate
uvicorn app:create_app --host 0.0.0.0 --port 8000
