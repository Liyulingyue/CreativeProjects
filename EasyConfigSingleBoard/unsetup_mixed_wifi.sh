#!/bin/bash

# unsetup_mixed_wifi.sh — 恢复 setup_mixed_wifi.sh 的更改
# - 停止并删除名为 `Hotspot` 的连接
# - 删除 SSID 以 EasyConfig- 开头、或 802-11-wireless.mode 为 ap 的保存连接
# - 可选 --purge：额外删除 /etc/NetworkManager/dispatcher.d 中包含 EasyConfig/Hotspot 的脚本
# Usage: sudo ./unsetup_mixed_wifi.sh [--purge] [--force]

set -euo pipefail
FORCE=0
PURGE=0

function usage() {
  cat <<'USAGE'
unsetup_mixed_wifi.sh — 恢复 setup_mixed_wifi.sh 的更改
Usage: sudo ./unsetup_mixed_wifi.sh [--purge] [--force]
  --purge    : 额外删除 /etc/NetworkManager/dispatcher.d 中包含 EasyConfig/Hotspot 的脚本（慎用）
  --force,-f : 不交互，直接执行
USAGE
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --purge) PURGE=1; shift ;;
    --force|-f) FORCE=1; shift ;;
    -h|--help) usage; exit 0 ;;
    *) echo "未知选项: $1"; usage; exit 1 ;;
  esac
done

if [[ $EUID -ne 0 ]]; then
  echo "错误：请使用 sudo 运行此脚本"
  exit 1
fi

echo "扫描 NetworkManager 中与 Hotspot / EasyConfig / AP 相关的连接..."

declare -a candidates
while IFS= read -r conn; do
  [[ -z "$conn" ]] && continue
  ssid=$(nmcli -g 802-11-wireless.ssid connection show "$conn" 2>/dev/null || true)
  mode=$(nmcli -g 802-11-wireless.mode connection show "$conn" 2>/dev/null || true)
  if [[ "$conn" == "Hotspot" ]] || [[ "$mode" == "ap" ]] || [[ "$ssid" == EasyConfig-* ]]; then
    candidates+=("$conn")
  fi
done < <(nmcli -t -f NAME connection show)

# 去重
mapfile -t to_delete < <(printf "%s\n" "${candidates[@]}" | awk '!seen[$0]++')

if [ ${#to_delete[@]} -eq 0 ]; then
  echo "未发现 Hotspot / EasyConfig / AP 模式的保存连接。"
else
  echo "以下连接将被删除:"
  for c in "${to_delete[@]}"; do echo "  - $c"; done
  if [ $FORCE -ne 1 ]; then
    read -p "确认删除以上连接？ (y/N) " ans
    case "$ans" in
      y|Y) ;;
      *) echo "未执行任何删除操作."; exit 0 ;;
    esac
  fi

  for c in "${to_delete[@]}"; do
    echo "停止并删除连接: $c"
    nmcli connection down "$c" 2>/dev/null || true
    nmcli connection delete "$c" 2>/dev/null || true
  done
fi

# 可选：清理 dispatcher 脚本
if [ $PURGE -eq 1 ]; then
  echo "查找 /etc/NetworkManager/dispatcher.d 中包含 EasyConfig/Hotspot 的脚本..."
  found=$(grep -IlE 'EasyConfig|Hotspot' /etc/NetworkManager/dispatcher.d/* 2>/dev/null || true)
  if [[ -n "$found" ]]; then
    echo "将删除下列 dispatcher 脚本:"
    echo "$found"
    if [ $FORCE -ne 1 ]; then
      read -p "确认删除这些文件？ (y/N) " ans
      case "$ans" in y|Y) ;; *) echo "跳过 dispatcher 清理."; found=""; esac
    fi
    if [[ -n "$found" ]]; then
      rm -f $found
      echo "已删除 dispatcher 脚本。"
    fi
  else
    echo "未找到相关 dispatcher 脚本。"
  fi
fi

echo "刷新 NetworkManager 状态并列出剩余相关连接..."
nmcli connection show | grep -E 'Hotspot|EasyConfig' || echo "  none"

echo "当前网络设备状态："
nmcli device status

echo "完成 — setup_mixed_wifi 的更改已还原（或已尝试还原）。"
exit 0
