# MiniCoder — 轻量自主编程代理（本文件夹说明）
这是 `MiniCoder` 子项目的说明（面向本目录的快速上手与设计说明）。

MiniCoder 目标：提供一个可交互、可扩展的本地编程代理，支持调用本地工具来查看/修改代码与运行环境，适合快速试验与开发流程自动化。

核心能力
- 自主迭代：Plan → Action → Verify 的闭环工作流程。
- 常用工具：`execute_bash`、`read_file`、`write_file`、`list_files`、`search_files`。
- 交互方式：通过 `mini_coder.py` 启动 REPL，实现多轮对话与工具调用。

快速开始
1. 安装依赖：
```bash
pip install -r requirements.txt
```
1. 配置环境变量（可选）：
```bash
cp .env.example .env
# 编辑 .env 填写 MODEL_KEY / MODEL_URL / MODEL_NAME 等
```
1. 启动交互式代理：
```bash
python mini_coder.py
```

TIP：仅用于本地测试时可以在 `llm_client.py` 使用 mock 或本地小模型，无需真实 API Key。

主要文件
- `mini_coder.py` — 交互式 CLI（入口）。
- `agent.py` — Agent 主逻辑：对话、工具调度与 LLM 协作。
- `tools.py` — 本地工具实现（读写文件、执行命令等）。
- `llm_client.py` — LLM 调用封装与适配。

MiniCoder vs MiniCoderMCP（简述）
- `MiniCoder`：工具通常以内嵌函数形式运行，适合快速开发与实验。
- `MiniCoderMCP`：将工具通过独立进程/服务暴露（IPC/HTTP），增强隔离性与可运维性，适合长期运行或多进程场景。

运行模式建议
- 开发/调试：直接用 `mini_coder.py` 启动 REPL，快速迭代。
- 稳定/生产：将工具迁移到长期运行的后端（例如 `FastAPI` + `uvicorn`），agent 改用 HTTP/WS 调用后端，提高稳定性与可观测性。

演进历史（简要）
- `origin_demo_v0.py`：最原始的 demo，来源参考文章：https://mp.weixin.qq.com/s/WPkCONFnBc84Q3V5Qjynrg。
- `v0/`：在 `origin_demo_v0.py` 基础上的改进与整理，保留早期架构与子 agent 思路。
- `v1/`：进一步重构，**移除了子 agent 概念**，采用更标准化的工具接口，更便于与 LLM 的 function-calling 集成。

建议阅读顺序：`origin_demo_v0.py` → `v0/` → `v1/`，方便理解设计演化与权衡。

想要我帮忙？
- 我可以把 `mcp_server.py` 改写为最小 `FastAPI` 常驻服务并给出 agent 连接示例；或将 agent 改为连接到外部常驻 MCP 服务。

---
(该 README 已根据本子目录做精简与聚焦，便于本地开发与后续迁移。）
