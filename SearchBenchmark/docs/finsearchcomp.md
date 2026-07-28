# FinSearchComp Benchmark

## 1. 官方信息

- **官方代码**: `references/FinSearchComp/`
- **数据集**: `references/finsearchcomp_data.json` (635 条)

## 2. 官方逻辑

```
数据集 → 模型回答 (可能需要 AkShare 实时数据) → Judge 评判 → 指标
```

官方评估 **金融搜索和推理能力**，需要实时市场数据。

## 3. 我们的实现逻辑

```
数据集 → Search API → RAG Agent → FinSearchCompGrader → 指标
```

我们评估 **Search API + RAG** 效果（简化版，无需 AkShare）。

## 4. 数据格式

```json
{
  "prompt_id": "(T2)Simple_Historical_Lookup_001",
  "prompt": "2024年全年中国外债净流入是多少？（单位：十亿美元）",
  "response_reference": "...",
  "judge_prompt_template": "...",
  "judge_system_prompt": "...",
  "label": [...]
}
```

## 5. 评测指标

| 指标 | 说明 |
|------|------|
| score | 0.0 ~ 1.0 (根据 judge 评分) |

## 6. 评测类型

| 类型 | 说明 |
|------|------|
| T1 | 需要实时数据 (AkShare) |
| T2 | 静态问题 |

## 7. 官方提示词复用

✅ **judge_system_prompt** - 完整复用官方中文评判系统提示词
✅ **judge_prompt_template** - 完整复用官方评判模板

评判逻辑：
- 识别学生答案的最终答案
- 与标准答案比对
- 分数只有 1 分（正确）和 0 分（错误）

## 8. 关键差异

| 维度 | 官方 FinSearchComp | SearchBenchmark |
|------|-------------------|-----------------|
| **目的** | 评估金融搜索推理 | 评估 Search API 效果 |
| **实时数据** | 需要 AkShare | 无需（简化版） |
| **评测类型** | T1 + T2 | T2 静态评测 |
| **Judge** | 官方 judge_prompt_template | 复用官方模板 |

## 8. 运行命令

```bash
python -m evals.finsearchcomp --info

python -m evals.finsearchcomp \
    --searcher-api-url "https://api.queri.com/search" \
    --searcher-api-key "env:QUERI_API_KEY" \
    --rag-model gpt-4o-mini \
    --rag-api-key "env:OPENAI_API_KEY" \
    --grader-model gpt-4o \
    --grader-api-key "env:OPENAI_API_KEY" \
    --num-results 5 \
    --output results/finsearchcomp.json
```
