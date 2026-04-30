---
name: ai-wallpaper-skill
description: 一个能够根据用户提示词生成 AI 图片并自动将其设置为桌面壁纸的能力模块。当用户提到“换壁纸”、“生成桌面”、“设置背景”等需求时触发。当前实现支持 Windows、macOS 和 Linux。
version: "1.1"
author: Liyulingyue & Copilot
license: MIT
metadata:
  environment: python3
  platform:
    - windows
    - macos
    - linux
  required-env-vars:
    BAIDU_API_KEY: "Baidu AIStudio AccessToken"
allowed-tools:
  - Bash(python3:*)
---

# Instructions

## 核心原则
- 这是一个面向 Windows、macOS 和 Linux 的桌面增强能力模块。
- 当前版本支持 Windows、macOS 和 Linux（Linux 需要 xrandr 命令支持）。
- 必须获取用户的 `prompt`（描述壁纸内容的提示词）。
- 必须获取 API 访问权限：
  - 提示用户从 [Baidu AIStudio控制台 (AccessToken)](https://aistudio.baidu.com/account/accessToken) 获取 Token。
  - 通过参数 `--api_key` 或环境变量 `BAIDU_API_KEY` 提供。
- Python 依赖为 `openai` 包；该脚本通过 OpenAI 兼容协议访问百度端点。
- 支持自动检测屏幕分辨率并选择最佳图片尺寸，也可以手动指定尺寸。

## 执行步骤
1. **环境检查**: 确认当前系统为 Windows、macOS 或 Linux。
2. **生成并配置**: 调用 `scripts/wallpaper_skill.py` 脚本，将用户的提示词作为输入。该脚本将自动执行以下操作：
   - 自动检测屏幕分辨率并选择最佳图片尺寸（或使用指定尺寸）。
   - 调用 AI API 生成图片。
   - 将图片保存至 `assets/wallpaper.png`。
   - 在 Windows 上通过 SPI 接口即时更新桌面壁纸；在 macOS 上通过 `osascript` 更新所有桌面空间的壁纸。

## 分辨率配置
脚本支持以下分辨率选项：
- **自动检测** (`--size auto`): 根据屏幕宽高比自动选择最佳尺寸（默认）
- **指定尺寸**: 直接指定图片尺寸，如 `--size 1024x1024`、`--size 1376x768` 等

支持的图片尺寸：
- 1024x1024 (正方形)
- 1376x768 (16:9 横屏)
- 1264x848 (3:2 横屏)
- 1200x896 (4:3 横屏)
- 768x1376 (9:16 竖屏)
- 848x1264 (2:3 竖屏)
- 896x1200 (3:4 竖屏)

## 输出反馈
- 成功后告知用户壁纸已更新，并提及图片已持久化存储在 assets/ 目录下。
