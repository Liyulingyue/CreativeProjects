---
name: ai-wallpaper-skill
description: 一个能够根据用户提示词生成 AI 图片并自动将其设置为桌面壁纸的能力模块。当用户提到“换壁纸”、“生成桌面”、“设置背景”等需求时触发。当前实现为 Windows 平台。
version: "1.0"
author: Liyulingyue & Copilot
license: MIT
metadata:
  environment: python3
allowed-tools:
  - Bash(python3:*)
---

# Instructions

## 核心原则
- 这是一个专门为 Windows 平台设计的桌面增强能力模块。
- 必须获取用户的 `prompt`（描述壁纸内容的提示词）。
- 必须获取 API 访问权限：
  - 提示用户从 [Baidu AIStudio控制台 (AccessToken)](https://aistudio.baidu.com/account/accessToken) 获取 Token。
  - 通过参数 `--api_key` 或环境变量 `BAIDU_API_KEY` 提供。

## 执行步骤
1. **环境检查**: 确认当前系统为 Windows。
2. **生成并配置**: 调用 `scripts/wallpaper_skill.py` 脚本，将用户的提示词作为输入。该脚本将自动执行以下操作：
   - 调用 AI API 生成图片。
   - 将图片保存至 `assets/wallpaper.png`。
   - 通过 Windows SPI 接口即时更新桌面壁纸。

## 输出反馈
- 成功后告知用户壁纸已更新，并提及图片已持久化存储在 assets/ 目录下。