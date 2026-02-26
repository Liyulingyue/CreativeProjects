# AgentLearn
本文件夹是根据开源教程
进行智能体、代码编辑器探索的实验代码
# AgentLearn

AgentLearn 是一个用于学习与实验智能体（Agent）、MCP（Model Context Protocol）与 Skill 设计的代码集合与教学示例。

## 目标
- 收集和实现用于理解智能体（agents）工作原理的示例与实验代码。
- 提供可复现的实验和小型参考实现，便于逐步演进为更完善的库或服务。

## 主要来源
- https://mp.weixin.qq.com/s/WPkCONFnBc84Q3V5Qjynrg
- https://datawhalechina.github.io/hello-agents/#/

## 目录概览（建议）
- `MiniCoder/` — 与代码编辑器 / 代码生成相关的示例与实验。
- `MiniCoderPlus/` — 扩展功能、前端示例与工作区整合。
- `MiniRAG/` — 小型 RAG（检索增强生成）示例。
- `requirements.txt` — 本目录参考依赖（如有）。

建议新增（可先在本仓库内作为子目录）：
- `MCP/` — MCP 概念、协议示例、实现与教程。
- `Skills/` — 各类 Skill 示例、接口规范与测试用例。

这些子目录适合先保留在 `AgentLearn` 下，便于与已有实验共享代码与数据；当某一部分成熟并独立用于其他项目时，再抽出为独立仓库更合适。

## 快速开始
1. 创建虚拟环境并安装依赖：

```bash
python -m venv .venv
.venv\Scripts\activate    # Windows
pip install -r requirements.txt
```

2. 运行示例或实验：各子目录通常包含 `README.md` 或 `run.py`，请进入相应子目录查看具体用法。

## 贡献与下一步
- 若你希望我现在在仓库中创建 `MCP/` 和 `Skills/` 子目录并添加模板 README 与示例文件，请选择 A（我会执行）。
- 如果只需一份详尽的组织与迁移建议文档，请选择 B。
- 如需先审阅现有子模块再决定，请选择 C。

----
更新时间：2026-02-26