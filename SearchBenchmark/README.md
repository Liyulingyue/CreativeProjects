# SearchBenchmark

评估自研 Search API (queri) 及竞对 Search API 在主流榜单上的效果。

**详细对比文档**：
- [SimpleQA](docs/simpleqa.md)
- [FreshQA](docs/freshqa.md)
- [FRAMES](docs/frames.md)
- [SealQA](docs/sealqa.md)
- [FinSearchComp](docs/finsearchcomp.md)

## 官方提示词复用

| 榜单 | 官方提示词 | 状态 |
|------|------------|------|
| SimpleQA | GRADER_TEMPLATE | ✅ 已复用 |
| FreshQA | FreshEval prompts | ✅ 已复用 |
| FinSearchComp | judge_prompt_template | ✅ 已复用 |

## 两种评测模式

### 1. Pipeline 模式（当前实现）

```
Search API → RAG Agent → Grader
```

Search API 先搜索，RAG Agent 综合结果回答，Grader 评判。

### 2. Agentic 模式（待扩展）

```
LLM (Agent) ← → Search API
       ↓
    Grader
```

LLM 主动决定何时调用搜索，可多次搜索后回答。

**当前实现**: Pipeline 模式
**未来扩展**: Agentic 模式（让 LLM 主动调用搜索）

## 已实现榜单

| 榜单 | 数据 | 评测指标 |
|------|------|----------|
| SimpleQA | 4326 条 | is_correct, accuracy_given_attempted, F1 |
| FreshQA | 600 条 | is_correct (YES/NO) |
| FRAMES | 824 条 | is_correct |
| SealQA | 需下载 | is_correct |
| FinSearchComp | 635 条 | score (0.0~1.0) |

## 统一参数

```bash
--searcher-api-url     # Search API 地址
--searcher-api-key    # Search API Key (env:VAR_NAME)
--rag-model           # RAG 模型名称
--rag-model-url       # RAG 模型 API 地址
--rag-api-key         # RAG 模型 API Key
--grader-model        # Grader 模型名称
--grader-model-url    # Grader 模型 API 地址
--grader-api-key      # Grader 模型 API Key
--num-results         # 搜索结果数量
--limit               # 限制评测数量
--output              # 输出文件
```

## 快速开始

```bash
# SimpleQA
python -m evals.simpleqa \
    --searcher-api-url "https://api.queri.com/search" \
    --searcher-api-key "env:QUERI_API_KEY" \
    --rag-model gpt-4o-mini --rag-api-key "env:OPENAI_API_KEY" \
    --grader-model gpt-4o --grader-api-key "env:OPENAI_API_KEY" \
    --output results/simpleqa.json

# FreshQA
python -m evals.freshqa \
    --searcher-api-url "https://api.queri.com/search" \
    --searcher-api-key "env:QUERI_API_KEY" \
    --rag-model gpt-4o-mini --rag-api-key "env:OPENAI_API_KEY" \
    --grader-model gpt-4o --grader-api-key "env:OPENAI_API_KEY" \
    --mode relaxed --output results/freshqa.json

# FRAMES
python -m evals.frames \
    --searcher-api-url "https://api.queri.com/search" \
    --searcher-api-key "env:QUERI_API_KEY" \
    --rag-model gpt-4o-mini --rag-api-key "env:OPENAI_API_KEY" \
    --grader-model gpt-4o --grader-api-key "env:OPENAI_API_KEY" \
    --output results/frames.json

# SealQA (需先下载数据)
python -c "from datasets import load_dataset; ds = load_dataset('vtllms/sealqa', name='seal_0', split='test'); ds.to_parquet('references/sealqa/seal-0.parquet')"
python -m evals.sealqa \
    --searcher-api-url "https://api.queri.com/search" \
    --searcher-api-key "env:QUERI_API_KEY" \
    --rag-model gpt-4o-mini --rag-api-key "env:OPENAI_API_KEY" \
    --grader-model gpt-4o --grader-api-key "env:OPENAI_API_KEY" \
    --config seal-0 --output results/sealqa.json

# FinSearchComp
python -m evals.finsearchcomp \
    --searcher-api-url "https://api.queri.com/search" \
    --searcher-api-key "env:QUERI_API_KEY" \
    --rag-model gpt-4o-mini --rag-api-key "env:OPENAI_API_KEY" \
    --grader-model gpt-4o --grader-api-key "env:OPENAI_API_KEY" \
    --output results/finsearchcomp.json
```
