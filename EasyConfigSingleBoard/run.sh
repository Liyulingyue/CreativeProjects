#!/bin/bash

# EasyConfig 运行脚本

# 检查 Python 环境
if ! command -v python3 &> /dev/null; then
    echo "错误: 未找到 python3"
    exit 1
fi

# 安装依赖
echo "正在检查依赖..."
pip3 install -r requirements.txt --quiet

# 运行 Web 服务
echo "正在启动 EasyConfig 服务..."
export FLASK_APP=app.py
python3 app.py
