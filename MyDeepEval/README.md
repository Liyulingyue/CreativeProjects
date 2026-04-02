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

### 基本用法

```python
from openai import OpenAI
from MyDeepEval import DeepEval

client = OpenAI(api_key="sk-...", base_url="...")

# 初始化时设定全局评估标准
de = DeepEval(
    client, 
    model="gpt-4",
    default_criteria=["事实准确性", "表达清晰度"]
)

# 模型输出与参考答案
model_output = "地球是圆的。"
reference_output = "地球是一个不规则的椭球体。"

# 执行极简评估
res = de.evaluate(model_output, reference_output)

if not res["error"]:
    print("评分:", res["data"]["score"])
    print("理由:", res["data"]["reason"])
```

## 打包与分发

1. `pip install build`
2. `python -m build`
