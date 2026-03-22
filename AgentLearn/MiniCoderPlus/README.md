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

## 🚀 混合部署模式 (推荐)

为了让 Agent 获得与主机完全一致的权限和路径访问能力，我们采用了 **“本地后端 + Docker 前端”** 的混合架构：
- **后端**：直接运行在物理机，拥有完整系统权限。
- **前端**：运行在 Docker 容器，通过 Nginx 转发请求。

### 一键部署

1. **配置环境**:
   复制 `.env.example` 为 `.env` 并填入您的 API 密钥。

2. **执行安装脚本**:
   ```bash
   chmod +x install_hybrid.sh
   ./install_hybrid.sh
   ```
   该脚本会自动：
   - 安装 Python 依赖。
   - 注册 `minicoder-backend.service` 系统服务（开机自启）。
   - 启动前端 Docker 容器。

3. **访问**:
   - **前端 Web 界面**: [http://localhost:20081](http://localhost:20081)
   - **后端 API 地址**: [http://localhost:20080](http://localhost:20080)

### 常用管理命令
- **查看后端日志**: `tail -f logs/backend.log`
- **重启后端**: `sudo systemctl restart minicoder-backend.service`
- **重启前端**: `docker compose restart frontend`
- **一键卸载**: `./uninstall_hybrid.sh`

## 💻 物理机手动启动 (调试用)

1. **启动交互式 CLI**:
   ```bash
   python main.py cli
   ```

2. **启动 Web API 服务**:
   ```bash
   python run.py server --port 20080
   ```

## 📂 文件架构

- `run.py` — **统一入口**，根据参数选择 CLI 或 Server 模式。
- `install_hybrid.sh` — **混合部署脚本**。
- `minicoder/` — **核心业务逻辑包**。
    - `agents/` — 🤖 **多角色代理目录**。
    - `tools.py` — 🛠️ 工具箱定义与实现。
    - `server/` — 🌐 FastAPI 实现的 Web 后端接口。
- `frontend/web/` — 🎨 **React+Vite 前端界面** (含 Nginx 配置)。
- `docker-compose.yml` — 🚀 仅用于启动前端容器。

---
