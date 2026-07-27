# SimpleQA Benchmark

## 1. 官方信息

- **官方代码**: `references/simple-evals/simpleqa_eval.py`
- **数据集**: `references/simple_qa_test_set.csv` (4326 条)

## 2. 官方逻辑

```
数据集 → LLM 直接回答 → Grader (CORRECT/INCORRECT/NOT_ATTEMPTED) → 指标
```

官方评估 **LLM 内部知识**，不使用搜索。

## 3. 我们的实现逻辑

```
数据集 → Search API → RAG Agent → Grader (复用官方模板) → 指标
```

我们评估 **Search API + RAG** 效果。

## 4. 数据格式

```json
{
  "question": "Who received the IEEE Frank Rosenblatt Award in 2010?",
  "answer": "Michio Sugeno",
  "metadata": {"topic": "Science and technology", "answer_type": "Person"}
}
```

## 5. 评测指标

| 指标 | 说明 |
|------|------|
| is_correct | 回答正确的比例 (A) |
| is_incorrect | 回答错误且矛盾的比例 (B) |
| is_not_attempted | 未回答/不知道的比例 (C) |
| accuracy_given_attempted | 已回答中的正确率 = is_correct / (is_correct + is_incorrect) |
| F1 | F1 = 2 * acc_given_att * is_correct / (acc_given_att + is_correct) |

## 6. 官方提示词复用

✅ **GRADER_TEMPLATE** - 完整复用官方评判提示词

评判逻辑：
- CORRECT (A): 预测答案包含目标答案的关键信息
- INCORRECT (B): 预测答案与目标答案矛盾
- NOT_ATTEMPTED (C): 预测答案未包含目标答案

## 7. 关键差异

| 维度 | 官方 SimpleQA | SearchBenchmark |
|------|---------------|-----------------|
| **目的** | 评估 LLM 内部知识 | 评估 Search API 效果 |
| **Search** | ❌ 无 | ✅ 有 |
| **Pipeline** | LLM → Grader | **Search API → RAG → Grader** |

## 7. 运行命令

```bash
python -m evals.simpleqa --info

python -m evals.simpleqa \
    --searcher-api-url "https://api.queri.com/search" \
    --searcher-api-key "env:QUERI_API_KEY" \
    --rag-model gpt-4o-mini \
    --rag-model-url "https://api.openai.com/v1" \
    --rag-api-key "env:OPENAI_API_KEY" \
    --grader-model gpt-4o \
    --grader-model-url "https://api.openai.com/v1" \
    --grader-api-key "env:OPENAI_API_KEY" \
    --num-results 5 \
    --output results/simpleqa.json
```
