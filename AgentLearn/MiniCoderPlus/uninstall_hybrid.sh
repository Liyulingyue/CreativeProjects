#!/bin/bash

# 获取当前绝对路径
BASE_DIR=$(cd "$(dirname "$0")" && pwd)

echo ">>> 开始卸载 MiniCoderPlus 部署..."

# 1. 停止并移除前端 Docker 容器
echo ">>> 停止并移除前端 Docker 容器..."
if [ -f "$BASE_DIR/docker-compose.yml" ]; then
    docker compose down
else
    echo "未找到 docker-compose.yml，跳过 Docker 清理。"
fi

# 2. 停止并禁用 systemd 后端服务
echo ">>> 停止并移除后端 systemd 服务..."
SERVICE_NAME="minicoder-backend.service"

if systemctl list-unit-files | grep -q "$SERVICE_NAME"; then
    sudo systemctl stop "$SERVICE_NAME"
    sudo systemctl disable "$SERVICE_NAME"
    sudo rm "/etc/systemd/system/$SERVICE_NAME"
    sudo systemctl daemon-reload
    echo "后端服务已移除。"
else
    echo "未找到后端服务 $SERVICE_NAME，跳过。"
fi

# 3. 清理日志文件 (可选，保留则注释掉)
# echo ">>> 清理日志文件..."
# rm -rf "$BASE_DIR/logs"

echo "===================================================="
echo "卸载完成！"
echo "提示: Python 依赖包 (pip install) 未自动卸载，如有需要请手动处理。"
echo "===================================================="
