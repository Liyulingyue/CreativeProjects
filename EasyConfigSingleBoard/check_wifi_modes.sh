#!/bin/bash

echo "=== WiFi 模式支持情况检测 ==="

# 检查是否安装了 iw
IW_PATH=$(command -v iw || which /usr/sbin/iw || which /sbin/iw)
if [ -z "$IW_PATH" ]; then
    echo "错误: 未找到 'iw' 工具。请尝试安装: sudo apt update && sudo apt install iw"
    exit 1
fi

# 获取默认网卡名称
WLAN=$($IW_PATH dev | grep Interface | awk '{print $2}' | head -n 1)
if [ -z "$WLAN" ]; then
    echo "警告: 未检测到活动的 WiFi 接口。"
else
    echo "检测到主接口: $WLAN"
fi

echo ""
echo "--- 支持的接口模式 (Supported interface modes) ---"
$IW_PATH list | sed -n '/Supported interface modes:/,/Band 1:/p' | grep -v "Band 1:" | sed 's/\t/*/g'

echo ""
echo "--- 接口组合支持 (Interface combinations) ---"
echo "这决定了是否可以同时运行 AP 和 STA 模式。"
$IW_PATH list | sed -n '/valid interface combinations:/,/Device accepts/p' | grep -v "Device accepts" | sed 's/\t/  /g'

echo ""
echo "--- 总结环境信息 ---"
if nmcli -v >/dev/null 2>&1; then
    echo "NetworkManager: 已安装"
else
    echo "NetworkManager: 未安装"
fi

if command -v rfkill >/dev/null 2>&1; then
    if rfkill list wifi | grep -q "yes"; then
        echo "注意: 硬件或软件 WiFi 开关可能已关闭 (rfkill blocked)"
    fi
else
    echo "rfkill: 未安装，跳过开关检查"
fi
