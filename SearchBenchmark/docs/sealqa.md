# SealQA Benchmark

## 1. 官方信息

- **官方代码**: HuggingFace (`vtllms/sealqa`) + Colab notebook
- **数据集**: 需从 HuggingFace 下载
  - `seal-0.parquet` (基础)
  - `seal-hard.parquet` (高难度)
  - `longseal.parquet` (长文本)

## 2. 官方逻辑

```
数据集 → w/o Search (内部知识) → 评判
       → w/ Search (搜索增强) → 评判
```

官方评估 **搜索结果冲突/噪声下的 QA 能力**，对比有无搜索的效果。

## 3. 我们的实现逻辑

```
数据集 → Search API → RAG Agent → Grader → 指标
```

我们评估 **Search API + RAG** 效果。

## 4. 数据格式

```json
{
  "question": "...",
  "answer": "...",
  "context": "..."
}
```

## 5. 评测指标

| 指标 | 说明 |
|------|------|
| is_correct | 答案是否正确 |

## 6. 配置

| 配置 | 说明 |
|------|------|
| seal-0 | 基础挑战集 |
| seal-hard | 更高难度 |
| longseal | 长文本挑战 |

## 7. 关键差异

| 维度 | 官方 SealQA | SearchBenchmark |
|------|-------------|-----------------|
| **目的** | 评估搜索结果质量对 QA 的影响 | 评估 Search API 效果 |
| **对比模式** | w/o Search vs w/ Search | 与官方一致 |
| **Grader** | Colab notebook | 自研 Grader |

## 8. 运行命令

```bash
# 下载数据
python -c "from datasets import load_dataset; ds = load_dataset('vtllms/sealqa', name='seal_0', split='test'); ds.to_parquet('references/sealqa/seal-0.parquet')"

python -m evals.sealqa --info

python -m evals.sealqa \
    --searcher-api-url "https://api.queri.com/search" \
    --searcher-api-key "env:QUERI_API_KEY" \
    --rag-model gpt-4o-mini \
    --rag-api-key "env:OPENAI_API_KEY" \
    --grader-model gpt-4o \
    --grader-api-key "env:OPENAI_API_KEY" \
    --config seal-0 \
    --num-results 5 \
    --output results/sealqa.json
```
