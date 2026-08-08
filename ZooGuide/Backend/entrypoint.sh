#!/bin/bash
set -e

cd /app

echo "[init] 初始化数据库..."
python -c "from app import db; db.init_db()"

echo "[init] 创建演示用户..."
python Backend/create_demo_user.py || true

echo "[start] 启动服务..."
exec python Backend/run.py
