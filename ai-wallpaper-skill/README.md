# Skill: ai-wallpaper-skill

遵循标准 AI Agent Skill 架构定义的桌面壁纸增强能力模块（当前实现仅支持 Windows）。

## 目录结构
- `SKILL.md`: **核心定义**。包含 YAML 元数据和 Markdown 执行指令。
- `scripts/`: 包含可执行脚本 `wallpaper_skill.py`。
- `assets/`: 存放生成的静态资源 `wallpaper.png`。

## 使用方式
1. **安装依赖**: `pip install -r requirements.txt`
2. **运行能力**:
   ```bash
   python scripts/wallpaper_skill.py --api_key "YOUR_KEY" --prompt "极光下的森林"
   ```

## 依赖与端点说明
- 脚本依赖 `openai` Python SDK。
- 默认 `base_url` 指向百度 AIStudio 的 OpenAI 兼容接口，因此会看到“OpenAI SDK + Baidu 端点”的组合。

## 设计特点
- **模块化**: 独立封装了从 AI 生成到系统底层调用的全流程。
- **按需加载**: 平时只保留 `SKILL.md` 中的元数据，仅在任务匹配时激活 Python 执行逻辑。
