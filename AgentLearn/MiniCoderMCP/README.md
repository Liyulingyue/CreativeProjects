# MiniCoderMCP — 简洁说明与快速上手

本文件为 `MiniCoder` 系列的 MCP（tool-calling）辅助层主说明，目标是：用最少但完整的信息，帮助开发者快速理解 v0 设计、启动流程、常见问题与向 v1 迁移方向。

## 核心概念（一句话）
v0 使用 JSON-over-stdio 的子进程模式把工具以元数据注册给代理（LLM），代理通过函数调用样式发起工具请求并接收结果。

## 快速上手（最短路径）
1) 在项目根创建并激活虚拟环境（PowerShell）：
```powershell
python -m venv .venv
& .venv\Scripts\Activate.ps1
pip install -r MiniCoderMCP/requirements.txt
```
2) （可选）手动启动服务以便查看日志：
```powershell
cd AgentLearn\MiniCoderMCP
python mcp_server.py
```
3) 启动 MiniCoder REPL：
```powershell
cd AgentLearn\MiniCoder
python mini_coder.py
```
说明：`mini_coder.py` 通常会在需要时自动以子进程方式启动 `mcp_server.py`。

## v0 设计速览
- 协议：JSON-over-stdio；工具以 JSON 元数据 `list_tools` 的形式被发现。
- 典型工具：`read_file`, `write_file`, `list_files`, `execute_bash`, `search_files`。
- 场景：本地调试、交互式 REPL、原型验证。

## 常见问题与快速解决
- Windows asyncio/proactor 报错（`'NoneType' object has no attribute 'send'`）：
  - 原因：事件循环或 proactor 被关闭但仍尝试写 pipe。解决：在 agent 端保持事件循环长期运行（后台线程 + `loop.run_forever()`，用 `asyncio.run_coroutine_threadsafe` 提交任务）。
- JSON 半包或消息边界问题：确保每条消息为完整 JSON 对象（建议以换行分隔），并通过 `MINICODER_LOG_LEVEL=DEBUG` 查看原始 stdin/stdout。
- 平台依赖命令（如 `grep`）：为 `search_files` 等工具准备跨平台实现或替代命令。

## 从 v0 到 v1（迁移建议，概要）
- 把 stdio 子进程通信替换为持久化后端（HTTP/WS）可提升稳定性与可观测性。推荐实现：FastAPI 提供 REST/WS 接口，agent 通过 HTTP 调用工具并保持长期连接。
- 同步加入认证/审计、限流与更细粒度的工具权限控制。

## 关键文件（参考）
- `AgentLearn/MiniCoderMCP/mcp_server.py` — MCP 服务（stdio 实现）。
- `AgentLearn/MiniCoderMCP/agent.py` — agent 端 MCP 客户端与运行逻辑。
- `AgentLearn/MiniCoder/mini_coder.py` — 上层 REPL / 调用入口。

## 我能帮你做的事（选项）
1. 把 `mcp_server` 做成 FastAPI PoC（包含 agent 示例）。
2. 在 `agent.py` 中加入 Windows 专用的长期事件循环示例并提供测试脚本。
3. 将本 README 再次缩短为 1 页速览文档（更偏向运维/部署）。

请选择要我接着做的项（1/2/3 或 指定其它修改）。

---

文件：`AgentLearn/MiniCoderMCP/README.md`

---

文件：`AgentLearn/MiniCoderMCP/README.md`
一个独立的 MCP 服务。代理可以生成代码并调用本地工具来查看目录、读写文件或执行命令。

**演进路径**

- origin_demo：最初的简单示例，来自一篇微信文章。  
- v0：从 MiniCoder/v1 改造而来，将工具抽离到 `mcp_server.py` 子进程。  
- v1：预期未来版本，计划引入 HTTP/WS 后端和安全策略。

**主要特征**

- MCP 协议（JSON-over-stdio）用于客户端/服务端通信。  
- 动态工具发现，适合与 LLM 的函数调用功能结合。  
- 支持交互式 REPL，方便快速试验。

## 通用安装与配置

```bash
pip install -r requirements.txt
cp .env.example .env   # 可选
```

## 快速启动

```bash
python mcp_server.py   # 可选，agent 会自动启动
python mini_coder.py
```

> 示例命令："创建脚本将所有 .docx 转为 .pdf 并运行测试"。

## 各版本说明

### v0（当前）

v0 是本项目的实际运行版本，安装与启动同上。详细特性见下文 "v0 特别说明"。

### v1（规划）

v1 尚未实现，设想增加：

- FastAPI/uvicorn 常驻后端；  
- 权限沙箱与审计；  
- 容器化/远程部署支持。

## 📂 文件说明

- `mini_coder.py` — 交互式 REPL（入口）。
- `agent.py` — Agent 主逻辑（对话、工具发现与调用）。
- `mcp_server.py` — 将工具注册为 MCP 接口并通过 stdio 提供服务。
- `tools.py` — 工具函数库（可被 `mcp_server.py` 调用）。
- `llm_client.py` — LLM 请求封装与适配。

---

## 🧪 联调测试

1. 在一个终端手动启动 MCP 服务器（或直接运行 `python mini_coder.py`，Agent 会自动启动它）。
2. 在另一个终端运行 `python mini_coder.py` 进入交互式 shell。
3. 在 REPL 中输入：
   > 列出当前目录文件

预期：Agent 将通过 MCP 发现 `list_files` 工具并调用，结果在屏幕上显示。

---

> ⚠️ 需要在环境变量中设置 `MODEL_KEY`（或写入 `.env`）。

## 演进与扩展计划

- `v0` 是从 `MiniCoder/v1` 改造而来：将进程内工具调用迁移为 MCP 架构以增强隔离与可扩展性。
- 后续计划包括：
  - 将 `mcp_server.py` 作为可选常驻后端（例如 `FastAPI` + `uvicorn`），通过 HTTP/WS 暴露工具接口；
  - 增加工具调用安全策略（白名单、路径限制、审计日志）；
  - 支持本地开发模式与远程/容器化部署模式切换；
  - 标准化工具接口，便于在不同运行环境中复用工具实现。

如需，我可以先提供一个最小 `FastAPI` PoC 并给出 agent 端的示例调用代码。

---

## v0 特别说明（基于 MiniCoder/v1 的改造）

v0 分支是从 `MiniCoder/v1` 演进而来，主要做法是把原来在进程内直接调用的工具迁移到独立的 MCP 进程中。下面是关于 v0 的安装与特殊注意事项：

- 安装：与主项目相同，运行：
  ```bash
  pip install -r requirements.txt
  ```

- 启动（两种方式）：
  - 开发快速方式（REPL 自动 spawn MCP）：
    ```bash
    python mini_coder.py
    ```
    Agent 在首次需要工具时会自动启动 `mcp_server.py` 子进程。
  - 手动启动 MCP 进程（便于调试/日志收集）：
    ```bash
    python mcp_server.py
    python mini_coder.py    # 在另一个终端
    ```

- 平台/兼容性注意：
  - Windows 上使用 asyncio + 子进程时要注意事件循环（Proactor）生命周期；若遇到 `NoneType`/`send` 错误，建议使用长期运行的事件循环或直接运行 MCP 常驻后端。
  - `search_files` 在 Windows 上依赖 `grep`，可能需要替换为跨平台实现（例如 Python 内置的文件扫描）。

- 调试建议：
  - 手动启动 `mcp_server.py` 可以直接看到 stderr/日志，便于排查工具内部异常；
  - 使用环境变量 `MINICODER_LOG_LEVEL=DEBUG` 打开更详细的 agent 日志（在 `agent.py` 中支持）。

- 特别设计点：
  - v0 保持了与 `v1` 的概念兼容性，但将工具调用解耦到独立进程，便于后续把工具迁移到 HTTP/WS 等长期后端。

