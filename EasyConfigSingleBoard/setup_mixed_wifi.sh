#!/bin/bash

# EasyConfig Mixed WIFI Setup Script
# Works on systems with NetworkManager (e.g., Raspberry Pi OS Bookworm)

HOTSPOT_SSID="EasyConfig-$(hostname)"
HOTSPOT_PASS="12345678"

echo "=== 配置混合WIFI模式 (AP + STA) ==="

# 0. 检查是否具有 root 权限
if [ "$EUID" -ne 0 ]; then
  echo "错误: 请使用 sudo 运行此脚本"
  exit 1
fi

# 1. 检查 NetworkManager 是否运行
if ! systemctl is-active --quiet NetworkManager; then
    echo "错误: NetworkManager 未运行。请确保系统已安装并使用 NetworkManager。"
    exit 1
fi

# 2. 检查是否有现有的 Hotspot 连接，如果有则删除
nmcli con delete Hotspot 2>/dev/null

# 3. 创建新的 AP 连接
echo "正在创建热点: $HOTSPOT_SSID ..."
# 不要在开机时自动连接 Hotspot；使用者可以用 `nmcli connection up Hotspot` 手动启动
nmcli con add type wifi ifname wlan0 con-name Hotspot autoconnect no ssid "$HOTSPOT_SSID"
nmcli con modify Hotspot 802-11-wireless.mode ap 802-11-wireless.band bg ipv4.method shared
nmcli con modify Hotspot wifi-sec.key-mgmt wpa-psk
nmcli con modify Hotspot wifi-sec.psk "$HOTSPOT_PASS"

# 4. 启动热点
echo "正在启动热点..."
nmcli con up Hotspot

echo "=== 配置完成 ==="
echo "热点 SSID: $HOTSPOT_SSID"
echo "热点密码: $HOTSPOT_PASS"
echo "默认 Web 管理页面: http://10.42.0.1:5000 (通常 NetworkManager 共享模式 IP 为 10.42.0.1)"
echo "注意: 如果硬件支持，您可以同时连接其他 WIFI。"
