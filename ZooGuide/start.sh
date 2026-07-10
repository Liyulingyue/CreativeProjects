#!/usr/bin/env bash
# Start backend and frontend in parallel
# Usage: ./start.sh [dev|prod]
set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BACKEND="$SCRIPT_DIR/Backend"
WEB="$SCRIPT_DIR/Web/PWA"

echo "=========================================="
echo "  ZooGuide 启动脚本"
echo "=========================================="
echo "Backend: $BACKEND"
echo "Web:     $WEB"
echo ""

# Check if .venv exists
if [ ! -d "$BACKEND/.venv" ]; then
  echo "[setup] 创建 Python 虚拟环境..."
  python3 -m venv "$BACKEND/.venv"
fi

# Install backend deps if needed
if [ ! -f "$BACKEND/.venv/.deps_installed" ]; then
  echo "[setup] 安装后端依赖..."
  source "$BACKEND/.venv/bin/activate"
  pip install -q -r "$BACKEND/requirements.txt"
  pip install -q -e "$SCRIPT_DIR/../OpenAIJsonWrapper" 2>/dev/null || true
  touch "$BACKEND/.venv/.deps_installed"
fi

# Create .env if not exists
if [ ! -f "$BACKEND/.env" ]; then
  echo "[setup] 创建 .env (USE_LLM=false, 规则引擎模式)..."
  cp "$BACKEND/.env.example" "$BACKEND/.env"
  # Replace placeholder key with real key
  if grep -q "OPENAI_API_KEY=" "$SCRIPT_DIR/../TravelSites/.env" 2>/dev/null; then
    echo "[setup] 从 TravelSites 复制 LLM 配置..."
    grep "OPENAI_API_KEY\|OPENAI_BASE_URL\|OPENAI_MODEL" "$SCRIPT_DIR/../TravelSites/.env" >> "$BACKEND/.env" 2>/dev/null || true
    sed -i 's/^USE_LLM=.*/USE_LLM=true/' "$BACKEND/.env"
  fi
fi

# Install frontend deps if needed
if [ ! -d "$WEB/node_modules" ]; then
  echo "[setup] 安装前端依赖 (可能需要几分钟)..."
  (cd "$WEB" && npm install)
fi

# Start backend
echo ""
echo "[start] 启动 Backend (port 8000)..."
source "$BACKEND/.venv/bin/activate"
cd "$BACKEND"
setsid python run.py > /tmp/zooguide-backend.log 2>&1 < /dev/null &
BACKEND_PID=$!
echo "[start] Backend PID: $BACKEND_PID"

# Start frontend
echo "[start] 启动 Web/PWA (port 5173)..."
cd "$WEB"
setsid npm run dev > /tmp/zooguide-web.log 2>&1 < /dev/null &
WEB_PID=$!
echo "[start] Web PID: $WEB_PID"

# Wait for services
sleep 5

echo ""
echo "=========================================="
echo "  启动完成"
echo "=========================================="
echo "Backend:  http://localhost:8000"
echo "API docs: http://localhost:8000/docs"
echo "Frontend: http://localhost:5173"
echo ""
echo "日志: tail -f /tmp/zooguide-backend.log"
echo "     tail -f /tmp/zooguide-web.log"
echo ""
echo "停止: pkill -f 'python run.py' && pkill -f 'vite'"
echo "=========================================="

# Save PIDs for cleanup
echo $BACKEND_PID > /tmp/zooguide-backend.pid
echo $WEB_PID > /tmp/zooguide-web.pid