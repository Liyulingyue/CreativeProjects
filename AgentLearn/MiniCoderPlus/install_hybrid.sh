#!/bin/bash

# 获取当前绝对路径
BASE_DIR=$(cd "$(dirname "$0")" && pwd)
VENV_DIR="$BASE_DIR/.venv"
USER=$(whoami)

echo ">>> 开始部署 MiniCoderPlus 本地后端..."

# 1. 检查并创建虚拟环境
if [ ! -d "$VENV_DIR" ]; then
    echo ">>> 创建虚拟环境 .venv..."
    python3 -m venv "$VENV_DIR"
fi

# 确认虚拟环境中的 python 路径
PYTHON_PATH="$VENV_DIR/bin/python"

# 检查虚拟环境是否创建成功 (有些精简系统 venv 虽创建目录但缺失二进制文件)
if [ ! -f "$PYTHON_PATH" ]; then
    echo ">>> 错误: 发现空虚拟环境。正在尝试重新创建并确保包含 pip..."
    rm -rf "$VENV_DIR"
    python3 -m venv "$VENV_DIR"
fi

# 检查并安装 pip
if ! $PYTHON_PATH -m pip --version > /dev/null 2>&1; then
    echo ">>> 虚拟环境中未发现 pip，尝试使用 ensurepip 自动修复..."
    $PYTHON_PATH -m ensurepip --upgrade || {
        echo ">>> ensurepip 失败。请运行以下命令安装必要的系统组件:"
        echo "    sudo apt update && sudo apt install -y python3-venv python3-pip"
        exit 1
    }
fi

# 2. 安装/更新 Python 依赖
echo ">>> 在虚拟环境中安装/更新依赖..."
$PYTHON_PATH -m pip install --upgrade pip -i https://pypi.tuna.tsinghua.edu.cn/simple
$PYTHON_PATH -m pip install -r "$BASE_DIR/requirements.txt" -i https://pypi.tuna.tsinghua.edu.cn/simple

# 3. 安装 PTY Bridge 的 Node.js 依赖
PTY_DIR="$BASE_DIR/minicoder/core/pty_bridge"
if [ -d "$PTY_DIR" ]; then
    echo ">>> 安装 PTY Bridge 依赖 (Node.js)..."
    if command -v npm >/dev/null 2>&1; then
        cd "$PTY_DIR" && npm install --mirror=https://registry.npmmirror.com
        cd "$BASE_DIR"
    else
        echo ">>> 警告: 未发现 npm，Web 终端可能无法正常工作。请手动安装 Node.js。"
    fi
fi

# 4. 创建目录 (如果需要)
mkdir -p "$BASE_DIR/logs"

# 4. 生成 systemd 服务文件
cat <<EOF | sudo tee /etc/systemd/system/minicoder-backend.service
[Unit]
Description=MiniCoderPlus Backend Service
After=network.target

[Service]
Type=simple
User=$USER
WorkingDirectory=$BASE_DIR
# 使用虚拟环境中的 python 启动
ExecStart=$PYTHON_PATH run.py server --host 0.0.0.0 --port 20080
Restart=always
StandardOutput=append:$BASE_DIR/logs/backend.log
StandardError=append:$BASE_DIR/logs/backend.error.log

[Install]
WantedBy=multi-user.target
EOF

# 4. 启动后端服务
echo ">>> 重载 systemd 并启动后端服务..."
sudo systemctl daemon-reload
sudo systemctl enable minicoder-backend.service
sudo systemctl restart minicoder-backend.service

# 5. 启动前端 Docker
echo ">>> 启动前端 Docker 容器 (映射到 20081)..."
# 使用 sudo 解决 Docker 权限问题，并兼容不同版本的 docker-compose
if command -v docker-compose >/dev/null 2>&1; then
    sudo docker-compose up -d --build frontend
else
    sudo docker compose up -d --build frontend
fi

echo "===================================================="
echo "部署完成！"
echo "后端状态: $(systemctl is-active minicoder-backend.service)"
echo "后端地址: http://localhost:20080 (本地访问)"
echo "前端地址: http://localhost:20081 (Web界面)"
echo "日志查看: tail -f $BASE_DIR/logs/backend.log"
echo "===================================================="
