# FRAMES Benchmark

> 参考: [Perplexity Search API 评测框架](https://github.com/search_evals) - 他们也使用 FRAMES 作为单步搜索评测 benchmark

## 1. 官方信息

- **官方代码**: 无官方 Python 代码
- **数据集**: `references/frames-benchmark.tsv` (824 条)
- **Perplexity 参考**: `search_evals` GitHub 仓库

## 2. 官方逻辑

```
数据集 → 模型多步推理 (需整合多个 Wikipedia 链接) → 评判
```

官方评估 **Multi-hop 推理能力**，需要多步推理。

## 3. 我们的实现逻辑

```
数据集 → Search API → RAG Agent → Grader → 指标
```

我们评估 **Search API + RAG** 效果。

## 4. Perplexity 评测结果参考

| Provider | SimpleQA | FRAMES |
|----------|----------|--------|
| Perplexity | 0.930 | 0.453 |
| Exa | 0.781 | 0.399 |
| Brave | 0.822 | 0.320 |
| SERP-based | 0.890 | 0.437 |

## 5. 数据格式

```json
{
  "id": 0,
  "prompt": "If my future wife has the same first name as the 15th first lady of the United States' mother...",
  "answer": "Jane Ballou",
  "wiki_links": ["https://en.wikipedia.org/wiki/...", ...],
  "reasoning_types": "Multiple constraints"
}
```

## 6. 评测指标

| 指标 | 说明 |
|------|------|
| is_correct | 答案是否正确 |

## 7. 推理类型

- Multiple constraints
- Numerical reasoning
- Tabular reasoning
- Multi-hop

## 8. 关键差异

| 维度 | 官方 FRAMES | SearchBenchmark |
|------|-------------|-----------------|
| **目的** | 评估 Multi-hop 推理 | 评估 Search API 效果 |
| **Search** | 不强制 | ✅ 有 |
| **Grader** | 无官方实现 | 自研 Grader |

## 9. 运行命令

```bash
python -m evals.frames --info

python -m evals.frames \
    --searcher-api-url "https://api.queri.com/search" \
    --searcher-api-key "env:QUERI_API_KEY" \
    --rag-model gpt-4o-mini \
    --rag-api-key "env:OPENAI_API_KEY" \
    --grader-model gpt-4o \
    --grader-api-key "env:OPENAI_API_KEY" \
    --num-results 5 \
    --output results/frames.json
```
