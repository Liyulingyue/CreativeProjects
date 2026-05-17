#!/bin/bash

# FastOpenCodeConfig - 自动化注册 systemd 服务脚本
# 用途：自动查找 opencode 路径并生成、安装 systemd 服务文件

SERVICE_NAME="opencode"
PORT=4096
HOSTNAME="0.0.0.0"

echo "正在检测 opencode 路径..."
OPENCODE_PATH=$(which opencode)

if [ -z "$OPENCODE_PATH" ]; then
    echo "错误：未找到 opencode 命令。请先运行 'curl -fsSL https://opencode.ai/install | bash' 安装。"
    exit 1
fi

echo "找到 opencode: $OPENCODE_PATH"

# 获取当前用户和工作目录
CURRENT_USER=$(whoami)
WORK_DIR=$PWD

# 读取账户和密码配置
read -p "请输入用户名 (留空使用默认值 opencode): " SERVER_USERNAME
SERVER_USERNAME=${SERVER_USERNAME:-opencode}

read -sp "请输入密码 (留空表示无密码保护): " SERVER_PASSWORD
echo ""

# 生成环境变量配置
ENVIRONMENT_CONFIG=""
if [ -n "$SERVER_USERNAME" ]; then
    ENVIRONMENT_CONFIG="${ENVIRONMENT_CONFIG}Environment=OPENCODE_SERVER_USERNAME=$SERVER_USERNAME"$'\n'
fi
if [ -n "$SERVER_PASSWORD" ]; then
    ENVIRONMENT_CONFIG="${ENVIRONMENT_CONFIG}Environment=OPENCODE_SERVER_PASSWORD=$SERVER_PASSWORD"$'\n'
fi

# 生成服务定义
SERVICE_CONTENT="[Unit]
Description=OpenCode Web Service
After=network.target

[Service]
Type=simple
User=$CURRENT_USER
WorkingDirectory=$WORK_DIR
ExecStart=$OPENCODE_PATH web --hostname $HOSTNAME --port $PORT
Restart=on-failure
RestartSec=5s
${ENVIRONMENT_CONFIG}[Install]
WantedBy=multi-user.target"

if [ -f "/etc/systemd/system/${SERVICE_NAME}.service" ]; then
    OVERWRITE_NOTICE="警告：检测到已存在的服务配置，重新安装将会覆盖。"
else
    OVERWRITE_NOTICE=""
fi

echo "生成的服务配置内容："
echo "--------------------------------"
echo "$SERVICE_CONTENT"
echo "--------------------------------"

# 询问是否应用
read -p "${OVERWRITE_NOTICE}是否将此配置安装到 /etc/systemd/system/${SERVICE_NAME}.service? (y/N) " confirm
if [[ $confirm != [yY] && $confirm != [yY][eE][sS] ]]; then
    echo "已取消安装。"
    exit 0
fi

# 写入临时文件并移动到系统目录
echo "$SERVICE_CONTENT" > /tmp/${SERVICE_NAME}.service
sudo mv /tmp/${SERVICE_NAME}.service /etc/systemd/system/${SERVICE_NAME}.service

# 加载并启动
echo "正在加载并启动服务..."
sudo systemctl daemon-reload
sudo systemctl enable ${SERVICE_NAME}.service
sudo systemctl restart ${SERVICE_NAME}.service

echo "--------------------------------"
echo "服务已成功注册并尝试启动！"
echo "状态检查：sudo systemctl status ${SERVICE_NAME}"
echo "查看日志：sudo journalctl -u ${SERVICE_NAME} -f"
echo "--------------------------------"
