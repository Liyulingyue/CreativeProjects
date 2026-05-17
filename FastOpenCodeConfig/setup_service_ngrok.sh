#!/bin/bash

# FastOpenCodeConfig - Ngrok 内网穿透自动化部署脚本
# 用途：仅为 OpenCode 配置 Ngrok 内网穿透，可单独使用或配合 setup_service.sh 使用

echo "=========================================="
echo "  OpenCode Ngrok 内网穿透配置"
echo "=========================================="
echo ""

echo "[1/5] 正在检测 ngrok 路径..."
NGROK_PATH=$(which ngrok)
if [ -z "$NGROK_PATH" ]; then
    echo "错误：未找到 ngrok 命令。请先参考 README_with_ngrok.md 安装。"
    exit 1
fi
echo "找到 ngrok: $NGROK_PATH"

echo ""
echo "[2/5] 正在检测 OpenCode 服务状态..."
if systemctl is-active --quiet opencode.service 2>/dev/null; then
    echo "检测到 opencode.service 已运行，正在使用现有服务"
    OPENCODE_RUNNING=true
elif systemctl is-active --quiet opencode 2>/dev/null; then
    echo "检测到 opencode 服务已运行，正在使用现有服务"
    OPENCODE_RUNNING=true
elif pgrep -f "opencode web" > /dev/null 2>&1; then
    echo "检测到 opencode 进程已运行，正在使用现有进程"
    OPENCODE_RUNNING=true
else
    echo "未检测到运行中的 OpenCode 服务或进程"
    OPENCODE_RUNNING=false
fi

echo ""
echo "[3/5] 请配置 Ngrok..."

read -p "  Ngrok 认证令牌 (必填): " NGROK_TOKEN
if [ -z "$NGROK_TOKEN" ]; then
    echo "错误：Ngrok 认证令牌不能为空。"
    exit 1
fi

read -p "  OpenCode 服务端口 (留空默认为 4096): " PORT
PORT=${PORT:-4096}

if [ "$OPENCODE_RUNNING" = "false" ]; then
    echo ""
    echo "[4/5] 是否需要同时配置 OpenCode systemd 服务？"
    echo "  1) 跳过，仅配置 Ngrok（稍后手动启动 OpenCode）"
    echo "  2) 运行 setup_service.sh 配置 OpenCode 服务"
    read -p "请选择 (1/2，留空默认选 1): " opencode_choice
else
    opencode_choice=""
fi

echo ""
echo "[5/5] 选择部署方式："
echo "  1) 安装为 systemd 服务（推荐，开机自启）"
echo "  2) 直接启动 Ngrok（仅当前会话生效）"
read -p "请选择 (1/2，留空默认选 1): " deploy_choice
deploy_choice=${deploy_choice:-1}

echo ""
echo "正在配置 Ngrok 认证令牌..."
$NGROK_PATH config add-authtoken $NGROK_TOKEN

if [ "$opencode_choice" = "2" ]; then
    echo ""
    echo "正在启动 OpenCode 服务配置..."
    if [ -x "./setup_service.sh" ]; then
        chmod +x ./setup_service.sh
        ./setup_service.sh
    else
        echo "错误：未找到 setup_service.sh 或无执行权限。"
        exit 1
    fi
    echo ""
fi

if [ "$deploy_choice" = "1" ]; then
    CURRENT_USER=$(whoami)
    WORK_DIR=$PWD

    NGROK_SERVICE_CONTENT="[Unit]
Description=Ngrok Tunnel for OpenCode
After=network.target opencode.service

[Service]
Type=simple
User=$CURRENT_USER
WorkingDirectory=$WORK_DIR
ExecStart=$NGROK_PATH http $PORT
Restart=on-failure
RestartSec=5s
Environment=NGROK_AUTHTOKEN=$NGROK_TOKEN

[Install]
WantedBy=multi-user.target"

    echo ""
    echo "========== opencode-ngrok.service =========="
    echo "$NGROK_SERVICE_CONTENT"
    echo ""

    read -p "是否安装 Ngrok 服务到 /etc/systemd/system/? (y/N) " confirm
    if [[ "$confirm" != "y" && "$confirm" != "Y" && "$confirm" != "yes" && "$confirm" != "Yes" ]]; then
        echo "已取消安装。"
        exit 0
    fi

    echo ""
    echo "正在安装 opencode-ngrok.service..."
    echo "$NGROK_SERVICE_CONTENT" > /tmp/opencode-ngrok.service
    sudo mv /tmp/opencode-ngrok.service /etc/systemd/system/opencode-ngrok.service

    echo "正在加载并启动服务..."
    sudo systemctl daemon-reload
    sudo systemctl enable opencode-ngrok.service
    sudo systemctl restart opencode-ngrok.service

    echo ""
    echo "=========================================="
    echo "  部署完成！"
    echo "=========================================="
    echo ""
    echo "查看 Ngrok 服务状态："
    echo "  sudo systemctl status opencode-ngrok"
    echo ""
    echo "查看 Ngrok 日志："
    echo "  sudo journalctl -u opencode-ngrok -f"
    echo ""
    echo "查看 Ngrok 公网地址："
    echo "  curl localhost:4040/api/tunnels"
else
    echo ""
    echo "正在直接启动 Ngrok..."
    echo ""
    echo "提示：按 Ctrl+C 停止 Ngrok"
    echo "=========================================="
    $NGROK_PATH http $PORT
fi

echo ""
echo "=========================================="
echo "  安全提醒"
echo "=========================================="
echo ""
echo "推送到公网务必配置账户密码保护！"
echo ""
echo "配置方法："
echo "  1. 通过 setup_service.sh 配置时会提示设置密码"
echo "  2. 或手动设置环境变量："
echo "     OPENCODE_SERVER_PASSWORD=your_password opencode web"
echo ""
echo "推荐环境变量："
echo "  OPENCODE_SERVER_USERNAME - 用户名 (默认 opencode)"
echo "  OPENCODE_SERVER_PASSWORD - 密码 (必填)"
echo "=========================================="
