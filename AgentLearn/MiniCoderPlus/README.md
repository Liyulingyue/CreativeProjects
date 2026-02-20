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

启动交互式 CLI：
```bash
python main.py cli
```

启动 Web API 服务（供 MiniCoderWeb 使用）：
```bash
python main.py server --port 8000
```

### 示例指令：
- "帮我创建一个 python 脚本，用于把当前目录下所有 .docx 转为 .pdf，并运行测试它。"
- "分析当前项目结构，并告诉我 tools.py 里有哪些函数。"

## 📂 文件架构

- `main.py` — **统一入口**，根据参数选择 CLI 或 Server 模式。
- `minicoder/` — **核心业务逻辑包**。
    - `agent.py` — 🤖 Agent 大脑，负责决策与推理。
    - `tools.py` — 🛠️ 工具箱定义与执行实现。
    - `llm_client.py` — 📡 统一的大模型 API 客户端。
    - `cli/` — 🖥️ CLI 专属交互界面逻辑。
    - `server/` — 🌐 FastAPI 实现的 Web 后端接口。

---
