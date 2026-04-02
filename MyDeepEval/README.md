# MyDeepEval

基于 `OpenAIJsonWrapper` 的轻量级评估库，提供面向 LLM 的评分/评价输出封装。

## 安装

由于本项目依赖于本地的 `OpenAIJsonWrapper`，请确保两者都已安装：

```bash
# 首先安装依赖包
pip install -e ../OpenAIJsonWrapper

# 然后安装本项目
pip install -e .
```

## 功能与用法

### 1. 基础评估 (DeepEval)
用于简单的模型输出与参考答案的对比评分。

```python
from openai import OpenAI
from MyDeepEval import DeepEval

client = OpenAI(...)
de = DeepEval(client, default_criteria=["事实准确性"])

res = de.evaluate("模型输出内容", "参考答案")
print(res["data"]["score"])
```

### 2. 原子评估 (DeepEvalQA)
用于评估任意两个文本元素（如 Q vs A, A vs A'）之间的关系。你可以多次调用并自行组合结果。

```python
from MyDeepEval import DeepEvalQA

de_qa = DeepEvalQA(client)

# Q: 问题, A: 回答/参考答案
# 场景 A: 评测模型回复 Q 的质量
res_q_vs_m = de_qa.evaluate(query="怎么减肥？", answer="每天跑步")

# 场景 B: 评测模型回复 M 与参考答案 R 的一致性
res_m_vs_r = de_qa.evaluate(query="每天跑步", answer="保持运动")

print(f"得分: {res_q_vs_m['data']['score']}")
```

## 打包与分发

1. `pip install build`
2. `python -m build`
