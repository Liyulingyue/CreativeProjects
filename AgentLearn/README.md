# AgentLearn

实验性仓库：用于学习与验证智能体（Agent）、MCP（Model Context Protocol）与 Skill 设计的参考实现与演示。

## 目标
- 提供可复现的小型示例与实验，帮助理解智能体如何与外部能力（工具/技能）交互。
- 为逐步演进为更稳定的库或服务留出清晰的迁移路径。

## 参考资料
- 微信文章：https://mp.weixin.qq.com/s/WPkCONFnBc84Q3V5Qjynrg
- Datawhale 教程：https://datawhalechina.github.io/hello-agents/#/

## 仓库结构（概要）
- `MiniCoder/` — 代码编辑/生成相关示例与 REPL。核心实验目录。
- `MiniCoderMCP/` — MCP stdio 服务与 agent 客户端实现（tool-calling 示例）。
- `MiniCoderPlus/` — 扩展功能、前端与集成示例。
- `MiniRAG/` — 检索增强生成（RAG）小型示例。
- 其余子项目：图像识别、OCR、部署示例等，按目录说明使用。

## 快速开始
在 Windows PowerShell 中（跨平台稍有差异）：

```powershell
python -m venv .venv
& .venv\Scripts\Activate.ps1
pip install -r requirements.txt
```

进入某个子模块运行示例，例如 `MiniCoder`：

```powershell
cd AgentLearn\MiniCoder
python mini_coder.py
```

注意：每个子目录通常带有自己的 `requirements.txt` 或 `README.md`，请先阅读该子目录文档以获取详细运行说明。

## 推荐新增目录（建议采纳）
- `MCP/`：集中放置协议说明、stdio 与 HTTP/WS 示例、测试用例与 PoC（推荐首先引入 FastAPI PoC）。
- `Skills/`：以可复用、可测试的方式组织 Skill 示例（文件操作、shell、搜索、编译等），并提供权限/审计示例。

## 贡献与选项
请选择下一步之一，我会据此执行：

- A — 我在仓库中创建 `MCP/` 与 `Skills/` 目录，并添加模板 `README.md` 与最小可运行示例。
- B — 我为仓库生成一份详尽的组织与迁移建议文档（如何从实验到生产）。
- C — 先审阅并摘要现有子模块（列出需要重构/补充的点），然后给出逐步计划。

回复 A / B / C 以继续。

----

更新时间：2026-02-27