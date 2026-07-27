# FreshQA Benchmark

## 1. 官方信息

- **官方代码**: `references/freshqa/` (Jupyter notebooks)
- **数据集**: `references/FreshQA_v112425 - freshqa.csv` (600 条)

## 2. 官方逻辑

```
数据集 → LLM 回答 (可使用 FreshPrompt 搜索增强) → FreshEval 评判 → 指标
```

官方评估 **LLM 实时知识更新能力**，问题有时效性。

## 3. 我们的实现逻辑

```
数据集 → Search API → RAG Agent → FreshQAGrader → 指标
```

我们评估 **Search API + RAG** 效果。

## 4. 数据格式

```json
{
  "id": "0",
  "question": "What is the name of the first animal to land on the moon?",
  "answers": ["No animal has ever landed on the moon yet.", "Neil Armstrong"],
  "effective_year": "before 2022",
  "false_premise": "TRUE",
  "num_hops": "one-hop",
  "fact_type": "slow-changing"
}
```

## 5. 评测指标

| 指标 | 说明 |
|------|------|
| is_correct | 答案是否正确 (YES/NO) |

## 6. 评测模式

| 模式 | 说明 |
|------|------|
| relaxed | 包含正确答案即正确 |
| strict | 必须精确匹配 |

## 7. 官方提示词复用

✅ **FreshEval 提示词** - 完整复用官方 FreshEval prompts

- Relaxed: 宽松评估，允许幻觉但主答案必须准确
- Strict: 严格评估，任何幻觉或过时信息都会导致错误

评判逻辑：
- 答案必须与标准答案匹配
- 对于数字答案，近似值通常不接受
- 对于虚假前提问题，必须指出虚假前提

## 8. 关键差异

| 维度 | 官方 FreshQA | SearchBenchmark |
|------|---------------|-----------------|
| **目的** | 评估 LLM 实时知识 | 评估 Search API 效果 |
| **多答案** | ✅ 支持 | ✅ 支持 |
| **评测模式** | Relaxed / Strict | Relaxed / Strict |

## 8. 运行命令

```bash
python -m evals.freshqa --info

python -m evals.freshqa \
    --searcher-api-url "https://api.queri.com/search" \
    --searcher-api-key "env:QUERI_API_KEY" \
    --rag-model gpt-4o-mini \
    --rag-api-key "env:OPENAI_API_KEY" \
    --grader-model gpt-4o \
    --grader-api-key "env:OPENAI_API_KEY" \
    --mode relaxed \
    --num-results 5 \
    --output results/freshqa.json
```
