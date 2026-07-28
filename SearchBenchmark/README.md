# SearchBenchmark

评估自研 Search API (queri) 及竞对 Search API 在主流榜单上的效果。

**参考**: [Perplexity Search API 评测框架](https://research.perplexity.ai/articles/architecting-and-evaluating-an-ai-first-search-api)

## 模式 (--mode)

| 模式 | 说明 | 依赖 |
|------|------|------|
| `simple` | 简化版本，直接 OpenAI SDK | openai |
| `langchain` | LangChain 版本，对齐 Tavily/web-search-api-evals | langchain, langchain-openai |

## 数据集准备

将数据集文件放到 `data/` 目录：

```
data/
├── .gitkeep                          # 确保目录可追踪
├── simpleqa/
│   └── simple_qa_test_set.csv      # 4326 条
├── freshqa/
│   └── FreshQA_v112425 - freshqa.csv # 600 条
├── frames/
│   └── frames-benchmark.tsv        # 824 条
├── sealqa/
│   ├── seal-0.parquet             # 111 条
│   ├── seal-hard.parquet          # 254 条
│   └── longseal.parquet           # 254 条
└── finsearchcomp/
    └── finsearchcomp_data.json    # 635 条
```

### 下载 SealQA 数据

```bash
HF_ENDPOINT=https://hf-mirror.com python -c "
from datasets import load_dataset
for name, fname in [('seal_0', 'seal-0.parquet'), ('seal_hard', 'seal-hard.parquet'), ('longseal', 'longseal.parquet')]:
    ds = load_dataset('vtllms/sealqa', name=name, split='test')
    ds.to_parquet(f'data/sealqa/{fname}')
    print(f'{fname}: {len(ds)} samples')
"
```

## 评测榜单

| 榜单 | 数据 | 评测指标 |
|------|------|----------|
| SimpleQA | 4326 条 | is_correct, accuracy_given_attempted, F1 |
| FreshQA | 600 条 | is_correct (YES/NO), relaxed/strict 模式 |
| FRAMES | 824 条 | is_correct |
| SealQA | 111/254 条 | is_correct |
| FinSearchComp | 635 条 | score (0.0~1.0) |

## 使用方法

### 查看数据集信息

```bash
# SimpleQA
python -m evals.simpleqa_mode --info

# FreshQA
python -m evals.freshqa_mode --info

# FRAMES
python -m evals.frames_mode --info

# SealQA
python -m evals.sealqa_mode --info

# FinSearchComp
python -m evals.finsearchcomp_mode --info
```

### SimpleQA

```bash
# 简化模式（默认）
python -m evals.simpleqa_mode \
    --mode simple \
    --searcher-api-url "https://api.queri.com/search" \
    --rag-model gpt-4o-mini --rag-api-key "env:OPENAI_API_KEY" \
    --grader-model gpt-4o --grader-api-key "env:OPENAI_API_KEY" \
    --output results/simpleqa_simple.json

# LangChain 模式
python -m evals.simpleqa_mode \
    --mode langchain \
    --searcher-api-url "https://api.queri.com/search" \
    --rag-model gpt-4o-mini --rag-api-key "env:OPENAI_API_KEY" \
    --grader-model gpt-4o --grader-api-key "env:OPENAI_API_KEY" \
    --output results/simpleqa_langchain.json
```

### FreshQA

```bash
# 简化模式（默认），relaxed 评测模式
python -m evals.freshqa_mode \
    --mode simple \
    --searcher-api-url "https://api.queri.com/search" \
    --rag-model gpt-4o-mini --rag-api-key "env:OPENAI_API_KEY" \
    --grader-model gpt-4o --grader-api-key "env:OPENAI_API_KEY" \
    --eval-mode relaxed \
    --output results/freshqa_relaxed.json

# 简化模式，strict 评测模式
python -m evals.freshqa_mode \
    --mode simple \
    --searcher-api-url "https://api.queri.com/search" \
    --rag-model gpt-4o-mini --rag-api-key "env:OPENAI_API_KEY" \
    --grader-model gpt-4o --grader-api-key "env:OPENAI_API_KEY" \
    --eval-mode strict \
    --output results/freshqa_strict.json

# LangChain 模式
python -m evals.freshqa_mode \
    --mode langchain \
    --searcher-api-url "https://api.queri.com/search" \
    --rag-model gpt-4o-mini --rag-api-key "env:OPENAI_API_KEY" \
    --grader-model gpt-4o --grader-api-key "env:OPENAI_API_KEY" \
    --output results/freshqa_langchain.json
```

### FRAMES

```bash
# 简化模式（默认）
python -m evals.frames_mode \
    --mode simple \
    --searcher-api-url "https://api.queri.com/search" \
    --rag-model gpt-4o-mini --rag-api-key "env:OPENAI_API_KEY" \
    --grader-model gpt-4o --grader-api-key "env:OPENAI_API_KEY" \
    --output results/frames_simple.json

# LangChain 模式
python -m evals.frames_mode \
    --mode langchain \
    --searcher-api-url "https://api.queri.com/search" \
    --rag-model gpt-4o-mini --rag-api-key "env:OPENAI_API_KEY" \
    --grader-model gpt-4o --grader-api-key "env:OPENAI_API_KEY" \
    --output results/frames_langchain.json
```

### SealQA

```bash
# 简化模式（默认）
python -m evals.sealqa_mode \
    --mode simple \
    --searcher-api-url "https://api.queri.com/search" \
    --rag-model gpt-4o-mini --rag-api-key "env:OPENAI_API_KEY" \
    --grader-model gpt-4o --grader-api-key "env:OPENAI_API_KEY" \
    --config seal-0 \
    --output results/sealqa_simple.json

# LangChain 模式
python -m evals.sealqa_mode \
    --mode langchain \
    --searcher-api-url "https://api.queri.com/search" \
    --rag-model gpt-4o-mini --rag-api-key "env:OPENAI_API_KEY" \
    --grader-model gpt-4o --grader-api-key "env:OPENAI_API_KEY" \
    --config seal-0 \
    --output results/sealqa_langchain.json

# 其他配置
--config seal-hard   # Seal-hard 配置
--config longseal    # Longseal 配置
```

### FinSearchComp

```bash
# 简化模式（默认）
python -m evals.finsearchcomp_mode \
    --mode simple \
    --searcher-api-url "https://api.queri.com/search" \
    --rag-model gpt-4o-mini --rag-api-key "env:OPENAI_API_KEY" \
    --grader-model gpt-4o --grader-api-key "env:OPENAI_API_KEY" \
    --output results/finsearchcomp_simple.json

# LangChain 模式
python -m evals.finsearchcomp_mode \
    --mode langchain \
    --searcher-api-url "https://api.queri.com/search" \
    --rag-model gpt-4o-mini --rag-api-key "env:OPENAI_API_KEY" \
    --grader-model gpt-4o --grader-api-key "env:OPENAI_API_KEY" \
    --output results/finsearchcomp_langchain.json
```

## 统一参数

| 参数 | 说明 | 默认值 |
|------|------|--------|
| `--mode` | 评测模式：`simple` 或 `langchain` | `simple` |
| `--searcher-api-url` | Search API 地址 | 必填 |
| `--searcher-api-key` | Search API Key（支持 `env:VAR_NAME`） | - |
| `--rag-model` | RAG 模型名称 | `gpt-4o-mini` |
| `--rag-api-key` | RAG 模型 API Key（支持 `env:VAR_NAME`） | - |
| `--grader-model` | Grader 模型名称 | `gpt-4o` |
| `--grader-api-key` | Grader 模型 API Key（支持 `env:VAR_NAME`） | - |
| `--num-results` | 搜索结果数量 | `5` |
| `--limit` | 限制评测数量 | - |
| `--concurrency` | 并发数量 | `10` |
| `--output`, `-o` | 输出文件 | - |

## 特定榜单参数

| 榜单 | 参数 | 说明 |
|------|------|------|
| FreshQA | `--eval-mode` | `relaxed` 或 `strict` |
| SealQA | `--config` | `seal-0`, `seal-hard`, 或 `longseal` |

## 官方提示词复用

| 榜单 | 官方提示词 | 状态 |
|------|------------|------|
| SimpleQA | GRADER_TEMPLATE | ✅ 已复用 |
| FreshQA | FreshEval prompts | ✅ 已复用 |
| FinSearchComp | judge_prompt_template | ✅ 已复用 |

## 参考实现

| 来源 | 说明 |
|------|------|
| Tavily Search Evals | SimpleQA 的 Grader prompt 参考 |
| web-search-api-evals (You.com) | 完整评测框架参考 |
| benchmarks/webcode-benchmark (Exa) | Pipeline 架构参考 |
| simple-evals (OpenAI) | 官方 prompt 和流程 |
