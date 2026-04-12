# OpenAIJsonWrapper

轻量级的 OpenAI 响应解析封装。

## 安装

您可以直接将本项目以文件夹形式提交到仓库，或通过本地路径安装：

```bash
pip install -e .
```

或通过 Git 直接安装：

```bash
pip install https://github.com/Liyulingyue/CreativeProjects/releases/download/OpenAIJsonWrapper/openai_json_wrapper-0.1.0-py3-none-any.whl
```

## 打包与分发

如果您需要生成可分发的二进制包（.whl）：

1. 安装打包工具：
   ```bash
   pip install build
   ```
2. 在项目根目录下执行打包：
   ```bash
   python -m build
   ```
   打包完成后，在 `dist/` 目录下会生成 `.whl` 和 `.tar.gz` 文件。

## 功能与用法

提供一个 `OpenAIJsonWrapper` 对象，自动注入 System Prompt 并解析模型输出的 JSON。

### 基本用法

```python
from openai import OpenAI
from OpenAIJsonWrapper import OpenAIJsonWrapper

client = OpenAI(api_key="sk-...", base_url="...")

# 定义需要的 JSON 结构
target_structure = {
    "user_info": {
        "name": "string",
        "age": "int",
        "hobbies": ["string"]
    },
    "summary": "string"
}

# 初始化 Wrapper (指定模型和结构)
wrapper = OpenAIJsonWrapper(client, model="gpt-4", target_structure=target_structure)

# 正常传入 messages，框架会自动注入 System Prompt
messages = [{"role": "user", "content": "你好，我是小明，今年 18 岁，喜欢打篮球和听歌。"}]

result = wrapper.chat(messages)

# 获取解析后的结果
if not result["error"]:
    print("解析后的数据:", result["data"])
    print("模型的思维链/正文内容:", result["reasoning"])
else:
    print("解析出错:", result["error"])
```

### 进阶功能

`OpenAIJsonWrapper` 支持在初始化或调用时传入 `background`（背景信息）和 `requirements`（特定需求）。

```python
# 在初始化时定义默认配置
wrapper = OpenAIJsonWrapper(
    client, 
    model="gpt-4o", 
    background="你是一个资深的简历分析专家。",
    requirements=["提取内容必须客观", "年龄若未知请填 0"]
)

# 在 chat 调用时覆盖或补充配置
result = wrapper.chat(
    messages, 
    target_structure=new_structure,      # 覆盖默认结构
    extra_requirements="补充一项新要求",   # 补充到默认需求中
    background="覆盖初始化时的背景信息"     # 覆盖默认背景
)
```

## 特性

- **自动提示词注入**: 自动在 System Prompt 中包含 JSON 结构定义、背景信息和需求说明。
- **DeepSeek 兼容**: 自动处理 `</think>` 标记，精准分离思维链内容。
- **健壮解析**: 使用正则表达式提取 ` ```json ` 块，具备自动容错（修复末尾逗号等）和回退提取（寻找最后一个合法的 JSON 对象/数组）能力。
- **灵活配置**: 支持在实例级别或调用级别设置结构、需求和背景，支持多需求合并。
- **极简设计**: 专注于 LLM 工具调用/结构化数据提取任务。
