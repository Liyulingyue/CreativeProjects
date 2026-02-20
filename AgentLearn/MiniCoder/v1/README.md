# MiniCoder Plus — 增强型自驱动 Agent 助理

MiniCoder Plus 是一个极简但功能强大的自主编程代理（Autonomous Agent），它不仅能生成代码，还能通过调用本地工具实时操作环境、阅读文件、执行命令并验证结果。

## 🎯 核心功能

- **自主迭代**：Agent 会根据您的指令进行规划、执行操作（Tool Use）并自检。
- **工具集成**：
    - `execute_bash`: 执行任意 Shell 命令。
    - `read_file` & `write_file`: 读写本地文件。
    - `list_files`: 智能列出目录结构（标记文件与文件夹）。
    - `search_files`: 基于 `grep` 的全量文本搜索。
- **交互式 Shell**：支持多轮对话、上下文记忆和实时思考过程（Thought）展示。

## 🚀 安装与设置

1. **安装依赖**:
   ```bash
   pip install -r requirements.txt
   ```

2. **环境变量**:
   复制 `.env.example` 为 `.env` 并配置您的 LLM 密钥：
   ```bash
   MODEL_KEY=your_api_key_here
   MODEL_URL=https://api.openai.com/v1 # 或您的代理地址
   MODEL_NAME=gpt-4o # 推荐使用具备强大 Function Calling 能力的模型
   ```

## 💻 快速开始

启动交互式 Shell：
```bash
python mini_coder.py
```

### 示例指令：
- "帮我创建一个 python 脚本，用于把当前目录下所有 .docx 转为 .pdf，并运行测试它。"
- "分析当前项目结构，并告诉我 tools.py 里有哪些函数。"
- "在当前目录搜索所有包含 'Error' 字符串的文件。"

## 📂 文件架构

- `mini_coder.py` — 项目入口，提供交互式 REPL 环境。
- `agent.py` — 核心 Agent 逻辑，负责任务拆解与工具调度。
- `tools.py` — 工具箱实现与模式定义（Schema）。
- `llm_client.py` — 轻量化 LLM 调用封装。

---

> ⚠️ 需要在环境变量中设置 `MODEL_KEY`（或写入 `.env`）。

## 文件说明

- `agent.py` — Agent 风格实现（业务逻辑）
- `llm_client.py` — LLM wrapper（集中化 API 调用）
- `mini_coder.py` — 极简 CLI（入口）
- `tools.py` — 辅助工具函数

---
