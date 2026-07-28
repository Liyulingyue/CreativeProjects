# SearchBenchmark

评测 Search API 在主流榜单上的效果，支持多种模式。

## 模式 (--mode)

| 模式 | 说明 | 依赖 |
|------|------|------|
| `simple` | 简化版本，直接 OpenAI SDK | openai |
| `langchain` | LangChain 版本，对齐 Tavily/web-search-api-evals | langchain, langchain-openai |

未来可扩展其他模式。

## 模式差异

### SimpleQA Grader Prompt

| 模式 | 来源 | 说明 |
|------|------|------|
| `simple` | 简化版 | 核心评判逻辑一致，更少 examples |
| `langchain` | Tavily/web-search-api-evals | 更多 examples |

## 已实现榜单

| 榜单 | 数据 | 状态 |
|------|------|------|
| SimpleQA | 4326 条 | ✅ |
| FreshQA | 600 条 | ✅ |
| FRAMES | 824 条 | ✅ |
| SealQA | 111/254 条 | ✅ |
| FinSearchComp | 635 条 | ✅ |

## 参考实现

| 来源 | 说明 |
|------|------|
| Tavily Search Evals | SimpleQA 的 Grader prompt 参考 |
| web-search-api-evals (You.com) | 完整评测框架参考 |
| benchmarks/webcode-benchmark (Exa) | Pipeline 架构参考 |
| simple-evals (OpenAI) | 官方 prompt 和流程 |

## 使用方法

```bash
# 简化版本（默认）
python -m evals.simpleqa --mode simple --searcher-api-url ... --grader-model gpt-4o

# LangChain 版本（对齐 Tavily）
python -m evals.simpleqa --mode langchain --searcher-api-url ... --grader-model gpt-4o
```
