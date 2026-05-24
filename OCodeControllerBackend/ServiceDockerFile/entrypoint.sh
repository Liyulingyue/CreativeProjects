#!/bin/bash
# FileBrowser 初始化脚本：容器启动时执行，只在数据库不存在时创建

FB_DB="/var/lib/filebrowser/filebrowser.db"
FB_USER="${FB_USERNAME:-admin}"
FB_PASS="${FB_PASSWORD:-admin123}"

mkdir -p /var/lib/filebrowser

if [ ! -f "$FB_DB" ]; then
    echo "[init] FileBrowser 数据库不存在，开始初始化..."
    /usr/local/bin/filebrowser config init \
        --address 0.0.0.0 \
        --port 8080 \
        --root / \
        --database "$FB_DB"
    /usr/local/bin/filebrowser users add "$FB_USER" "$FB_PASS" \
        --perm.admin \
        --database "$FB_DB"
    echo "[init] FileBrowser 用户 ${FB_USER} 创建完成"
else
    echo "[init] FileBrowser 数据库已存在，跳过初始化"
fi

# 启动 supervisor
exec /usr/bin/supervisord -c /etc/supervisor/conf.d/supervisord.conf
