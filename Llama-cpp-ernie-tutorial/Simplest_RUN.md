# 傻瓜式教程：光速在CPU上开启Ernie LLM服务（基于llama.cpp编译结果）

## 引言
想在CPU上超快开启Ernie LLM服务？这个教程使用预编译的llama.cpp二进制文件，从ModelScope或Hugging Face下载GGUF模型。

## 前提条件
- CPU电脑

## 步骤1：下载llama.cpp编译产物
去GitHub下载预编译的llama.cpp：https://github.com/ggerganov/llama.cpp/releases

选择最新release，根据你的系统下载对应的二进制文件包（比如llama-bin-win-*.zip或llama-bin-linux-*.tar.gz）。

解压到文件夹，比如`./llama.cpp`。

## 步骤2：下载Ernie GGUF模型
从ModelScope下载（推荐，中国用户快）：
```bash
pip install modelscope
modelscope download --model unsloth/ERNIE-4.5-0.3B-PT-GGUF ERNIE-4.5-0.3B-PT-Q4_K_M.gguf --local_dir ./ernie_model
```

或者从Hugging Face下载：
```bash
pip install huggingface_hub
hf download unsloth/ERNIE-4.5-0.3B-PT-GGUF ERNIE-4.5-0.3B-PT-Q4_K_M.gguf --local-dir ./ernie_model
```


## 步骤3：启动服务
进入llama.cpp文件夹，运行：

```bash
./llama-server --model ../ernie_model/ernie.gguf --host 0.0.0.0 --port 8000
```

（Windows用`llama-server.exe`，路径调整为`..\ernie_model\ernie.gguf`）

服务立即启动！

## 步骤4：测试服务
先安装OpenAI库：
```bash
pip install openai
```

然后用Python测试（保存为test.py并运行）：
```python
from openai import OpenAI

client = OpenAI(
    api_key="sk-1234567890",  # 随便填
    base_url="http://localhost:8000/v1"
)

response = client.chat.completions.create(
    model="ernie",
    messages=[{"role": "user", "content": "你好"}]
)

print(response.choices[0].message.content)
```

看到回复就成功了！

