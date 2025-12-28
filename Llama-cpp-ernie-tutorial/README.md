# llama-cpp-ernie-tutorial

本教程演示如何将 ERNIE 4.5 模型转换为 GGUF 格式，并使用 llama.cpp 启动兼容 OpenAI API 的服务。（教程中的脚本代码见末尾）

## 概述

ERNIE 4.5 是百度开发的强大语言模型。本教程将展示如何：

1. 将 ERNIE 4.5 模型转换为 GGUF 格式
2. 使用 llama.cpp 启动本地服务
3. 通过 OpenAI 兼容的 API 调用模型

## 前提条件

- Python 3.8+
- Git (用于编译 llama.cpp)
- CMake (用于编译 llama.cpp)
- C/C++ 编译器(用于编译 llama.cpp)：Windows 用户需要安装 MinGW
- llama.cpp (需要克隆和编译)

## 前置安装指南

### Ubuntu

在 Ubuntu 上，您需要使用 `apt` 包管理器安装必要的依赖项。运行以下命令：

```bash
sudo apt update
sudo apt install python3 python3-pip python3-venv git cmake build-essential
```

> **注意：** 上述安装可能并不完整，具体取决于您的Ubuntu版本和系统配置。请根据需要安装其他必要的组件，例如特定版本的Python或额外的构建工具。

### Windows
### 安装 CMake

1. **使用 winget（推荐，Windows 10+）：**
   ```powershell
   winget install Kitware.CMake
   ```

2. 验证安装：打开命令提示符或 PowerShell，运行 `cmake --version` 确认安装成功。

安装完成后，请重新打开终端以确保 PATH 更新。

### 安装 MinGW

1. 从 [WinLibs](https://winlibs.com/#download-release) 下载最新的 MinGW-w64 预编译版本（选择 x86_64-posix-seh 版本），解压到合适目录（如 `C:\mingw64`）。

2. 将 MinGW 的 `bin` 目录添加到系统 PATH 环境变量中：
   - 右键点击“此电脑” > “属性” > “高级系统设置” > “环境变量”
   - 在“系统变量”中找到“Path”，点击“编辑”
   - 点击“新建”，添加路径 `C:\mingw64\bin`（根据您的安装目录调整）
   - 点击“确定”保存更改

**可选：通过命令行添加 PATH（需要管理员权限，重启终端后生效）：**
- 在 PowerShell 中（以管理员身份运行）：
  ```powershell
  [Environment]::SetEnvironmentVariable("Path", [Environment]::GetEnvironmentVariable("Path", "Machine") + ";C:\mingw64\bin", "Machine")
  ```
- 或在命令提示符中（以管理员身份运行）：
  ```cmd
  setx PATH "%PATH%;C:\mingw64\bin" /M
  ```

3. 验证安装：打开新的命令提示符或 PowerShell 窗口，运行 `gcc --version` 确认 GCC 已安装。

## 方式一：从源码编译 llama.cpp 并转换模型

### 0. 创建虚拟环境

为了避免依赖冲突，建议使用 Python 虚拟环境：

```bash
python -m venv .venv
```

激活虚拟环境：
- Windows (PowerShell)：
  ```powershell
  .\.venv\Scripts\Activate.ps1
  ```
- Windows (命令提示符)：
  ```cmd
  .\.venv\Scripts\activate.bat
  ```
- Linux/macOS：
  ```bash
  source .venv/bin/activate
  ```

**注意：** 如果 PowerShell 执行策略阻止运行脚本，请以管理员身份运行 PowerShell 并执行 `set-ExecutionPolicy RemoteSigned`。

### 1. 安装依赖

```bash
pip install -r requirements.txt
```

### 2. 克隆和编译 llama.cpp

```bash
git clone https://github.com/ggerganov/llama.cpp
cd llama.cpp
mkdir build
cd build
cmake .. 
# Windows 下可能的编译配置方式是 cmake .. -G "MinGW Makefiles" -DLLAMA_CURL=OFF 
# 或 cmake .. -G "MinGW Makefiles" -DLLAMA_CURL=OFF -DCMAKE_CXX_FLAGS="-D_WIN32_WINNT=0x0A00"
cmake --build . --config Release
# Windows 下可能的编译方式是 mingw32-make
# 回到根目录
cd ../.. 
```

> 在笔者的两台Windows电脑上可能因为配置不同导致操作不完全一致，请根据自己的电脑情况调整命令

### 3. 下载 ERNIE 4.5 模型

从 Hugging Face 下载 ERNIE 4.5 模型权重：

```bash
# 示例：下载 ERNIE-4.5-0.3B-Instruct
hf download baidu/ERNIE-4.5-0.3B-PT --local-dir ./ernie_model_03
```

或者从魔搭社区下载（可选）：

```bash
# 安装 modelscope（如果未安装）
pip install modelscope

# 示例：下载 ERNIE-4.5-0.3B-Instruct
modelscope download --model PaddlePaddle/ERNIE-4.5-0.3B-PT --local_dir ./ernie_model_03
```

### 4. 转换为 GGUF 格式
```bash
# 安装 convert_hf_to_gguf.py 的依赖
pip install -r llama.cpp/requirements/requirements-convert_hf_to_gguf.txt
python llama.cpp/convert_hf_to_gguf.py ./ernie_model_03 --outfile ./ernie.gguf --outtype f16

> **故障排除：** Windows 环境下如果在转换过程中遇到 PyTorch DLL 加载错误（如 "动态链接库(DLL)初始化例程失败"），请确保安装了 Microsoft Visual C++ Redistributable for Visual Studio 2015-2019 (x64)，可以从 Microsoft 官网下载。

### 5. 量化
```bash
# 使用 quantize 工具对已转换的 GGUF 文件进行量化
# 示例：将 f16 模型量化到 Q4_K_M
./llama.cpp/build/bin/llama-quantize.exe ./ernie.gguf ./ernie_q4_k_m.gguf Q4_K_M

# 其他量化示例：
# 量化到 Q8_0 (推荐用于平衡大小和性能)
./llama.cpp/build/bin/llama-quantize.exe ./ernie.gguf ./ernie_q8_0.gguf Q8_0

# 量化到 Q4_K_S (更小的 Q4 变体)
./llama.cpp/build/bin/llama-quantize.exe ./ernie.gguf ./ernie_q4_k_s.gguf Q4_K_S
```

> **注意：** 量化会减少模型大小并可能提高推理速度，但可能会略微降低模型精度。Q4_K_M 是常用的平衡选择。

## 方式二：使用预编译文件和下载 GGUF 模型

如果你不想从源码编译，可以下载预编译的 llama.cpp 和已经转换好的 GGUF 模型。

### 下载预编译的 llama.cpp

从 [llama.cpp releases](https://github.com/ggml-org/llama.cpp/releases) 下载最新版本的预编译二进制文件。根据你的操作系统选择合适的版本，解压到项目目录。

> **注意：** 后续启动命令是基于源码编译配置的，如果您使用预编译版本，请确保二进制文件路径正确，并相应调整启动命令中的路径（例如，将 `./llama.cpp/build/bin/server` 替换为预编译二进制文件的实际路径）。

### 下载转换好的 GGUF 模型

从 Hugging Face 或 ModelScope 下载已经转换好的 ERNIE GGUF 模型：

- Hugging Face: 搜索 "ERNIE GGUF"
- ModelScope: 搜索 "ERNIE GGUF"

将下载的 `.gguf` 文件放置在项目根目录。

> **注意：** 后续启动命令是基于源码编译和转换配置的，请调整启动时的模型路径到您的下载路径。

## 启动服务和测试

> **注意：** 如果你创建了虚拟环境，请确保先激活它：
> - Windows (PowerShell)：`.\.venv\Scripts\Activate.ps1`
> - Windows (命令提示符)：`.\.venv\Scripts\activate.bat`
> - Linux/macOS：`source .venv/bin/activate`

### 1. 启动服务

使用 llama.cpp 的服务器：

```bash
./llama.cpp/build/bin/llama-server --model ./ernie_q4_k_m.gguf --host 0.0.0.0 --port 8000 --ctx-size 4096
```

或者使用我们提供的 server.py 脚本（推荐），它封装了默认参数配置，无需每次都输入长命令：

```bash
python server.py
```

如果需要自定义参数，可以使用命令行选项，例如：

```bash
python server.py --model ./custom.gguf --port 8080
```

### 2. 测试调用

使用客户端脚本测试（脚本已封装默认配置，可以直接运行测试，或指定自定义参数）：

```bash
# 使用 requests 库的客户端（默认URL: http://localhost:8000，默认提示: "你好，请介绍一下自己。"）
python client_request.py

# 使用 OpenAI 库的客户端（需要先安装 openai 库：pip install openai，默认URL: http://localhost:8000，默认提示: "你好，请介绍一下自己。"）
python client_openai.py

# 示例：指定自定义提示或参数
python client_request.py --prompt "请解释什么是机器学习"
python client_request.py --url http://localhost:8080 --model custom-model --temperature 0.5
```

## 文件说明
- `requirements.txt`: Python 依赖
  ```text
  # llama-cpp-ernie-tutorial 依赖

  # 使用清华镜像源加速安装
  --index-url https://pypi.tuna.tsinghua.edu.cn/simple/

  # Transformers 和相关库
  transformers>=4.35.0
  sentencepiece>=0.1.99

  # Web 服务 (如果需要自定义服务器)
  fastapi>=0.104.0
  uvicorn[standard]>=0.24.0

  # OpenAI 客户端库
  openai>=1.0.0

  # 其他工具
  huggingface_hub>=0.17.0
  requests>=2.28.0
  ```
- `server.py`: 启动 llama.cpp 服务器的脚本
  ```python
  #!/usr/bin/env python3
  """
  ERNIE GGUF 服务器启动脚本
  启动 llama.cpp 的 server 二进制以提供 OpenAI 兼容的 API
  """

  import argparse
  import os
  import subprocess
  import sys
  from pathlib import Path

  def start_server(model_path: str, llama_cpp_path: str = "./llama.cpp",
                  host: str = "0.0.0.0", port: int = 8000, ctx_size: int = 4096,
                  n_threads: int = 4):
      """
      启动 llama.cpp server

      Args:
          model_path: GGUF 模型文件路径
          llama_cpp_path: llama.cpp 仓库路径
          host: 服务器主机
          port: 服务器端口
          ctx_size: 上下文大小
          n_threads: CPU 线程数
      """

      # 检查模型文件
      if not Path(model_path).exists():
          print(f"错误: 模型文件不存在: {model_path}")
          return False

      # 检查 server 二进制
      server_binary = Path(llama_cpp_path) / "build" / "bin" / "llama-server.exe"
      if not server_binary.exists():
          print(f"错误: llama.cpp server 二进制未找到: {server_binary}")
          print("请确保已编译 llama.cpp")
          return False

      print(f"启动 llama.cpp server...")
      print(f"模型: {model_path}")
      print(f"主机: {host}:{port}")
      print(f"上下文大小: {ctx_size}")
      print(f"线程数: {n_threads}")

      # 构建命令
      cmd = [
          str(server_binary),
          "--model", model_path,
          "--host", host,
          "--port", str(port),
          "--ctx-size", str(ctx_size),
          "--threads", str(n_threads)
      ]

      print(f"执行命令: {' '.join(cmd)}")

      try:
          # 启动服务器（阻塞）
          subprocess.run(cmd, cwd=llama_cpp_path)
          return True
      except KeyboardInterrupt:
          print("\n服务器已停止")
          return True
      except Exception as e:
          print(f"启动服务器失败: {e}")
          return False

  def main():
      parser = argparse.ArgumentParser(description="ERNIE GGUF 服务器启动器")
      parser.add_argument("--model", default="./ernie_q4_k_m.gguf", help="GGUF 模型文件路径")
      parser.add_argument("--llama-cpp", default="./llama.cpp", help="llama.cpp 仓库路径")
      parser.add_argument("--host", default="0.0.0.0", help="服务器主机")
      parser.add_argument("--port", type=int, default=8000, help="服务器端口")
      parser.add_argument("--ctx-size", type=int, default=4096, help="上下文大小")
      parser.add_argument("--threads", type=int, default=4, help="CPU 线程数")

      args = parser.parse_args()

      success = start_server(
          model_path=args.model,
          llama_cpp_path=args.llama_cpp,
          host=args.host,
          port=args.port,
          ctx_size=args.ctx_size,
          n_threads=args.threads
      )

      if not success:
          sys.exit(1)

  if __name__ == "__main__":
      main()
  ```
- `client_request.py`: 使用 requests 库的测试客户端
  ```python
  #!/usr/bin/env python3
  """
  ERNIE GGUF API 客户端测试工具
  """

  import argparse
  import json
  import requests
  import sys

  def chat_completion(url: str, prompt: str, model: str = "ernie-gguf",
                    max_tokens: int = 1024, temperature: float = 0.7, stream: bool = False):
      """发送聊天完成请求"""

      payload = {
          "model": model,
          "messages": [
              {"role": "user", "content": prompt}
          ],
          "max_tokens": max_tokens,
          "temperature": temperature,
          "stream": stream
      }

      headers = {"Content-Type": "application/json"}

      try:
          if stream:
              # 流式响应
              response = requests.post(f"{url}/v1/chat/completions",
                                    json=payload, headers=headers, stream=True)

              if response.status_code != 200:
                  print(f"错误: {response.status_code}")
                  print(response.text)
                  return

              print("响应:")
              for line in response.iter_lines():
                  if line:
                      line = line.decode('utf-8')
                      if line.startswith('data: '):
                          data = line[6:]
                          if data == '[DONE]':
                              break
                          try:
                              chunk = json.loads(data)
                              if chunk["choices"]:
                                  content = chunk["choices"][0]["delta"].get("content", "")
                                  print(content, end="", flush=True)
                          except json.JSONDecodeError:
                              continue
              print("\n")
          else:
              # 非流式响应
              response = requests.post(f"{url}/v1/chat/completions",
                                    json=payload, headers=headers)

              if response.status_code == 200:
                  result = response.json()
                  content = result["choices"][0]["message"]["content"]
                  print("响应:")
                  print(content)
                  print(f"\n使用 tokens: {result['usage']['total_tokens']}")
              else:
                  print(f"错误: {response.status_code}")
                  print(response.text)

      except requests.exceptions.RequestException as e:
          print(f"请求失败: {e}")

  def list_models(url: str):
      """列出可用模型"""
      try:
          response = requests.get(f"{url}/v1/models")
          if response.status_code == 200:
              models = response.json()
              print("可用模型:")
              for model in models["data"]:
                  print(f"- {model['id']} (by {model['owned_by']})")
          else:
              print(f"错误: {response.status_code}")
              print(response.text)
      except requests.exceptions.RequestException as e:
          print(f"请求失败: {e}")

  def main():
      parser = argparse.ArgumentParser(description="ERNIE GGUF API 客户端")
      parser.add_argument("--url", default="http://localhost:8000", help="API 服务器 URL")
      parser.add_argument("--prompt", default="你好，请介绍一下自己。", help="要发送的提示文本")
      parser.add_argument("--model", default="ernie-gguf", help="模型名称")
      parser.add_argument("--max-tokens", type=int, default=1024, help="最大生成 token 数")
      parser.add_argument("--temperature", type=float, default=0.7, help="温度参数")
      parser.add_argument("--stream", action="store_true", help="启用流式响应")
      parser.add_argument("--list-models", action="store_true", help="列出可用模型")

      args = parser.parse_args()

      if args.list_models:
          list_models(args.url)
      else:
          chat_completion(
              url=args.url,
              prompt=args.prompt,
              model=args.model,
              max_tokens=args.max_tokens,
              temperature=args.temperature,
              stream=args.stream
          )

  if __name__ == "__main__":
      main()
  ```
- `client_openai.py`: 使用 OpenAI 库的测试客户端
  ```python
  #!/usr/bin/env python3
  """
  ERNIE GGUF OpenAI API 客户端测试工具
  使用 OpenAI 库调用本地服务器
  """

  import argparse
  import sys
  from openai import OpenAI

  def chat_completion(base_url: str, prompt: str, model: str = "ernie-gguf",
                    max_tokens: int = 1024, temperature: float = 0.7, stream: bool = False):
      """发送聊天完成请求"""

      client = OpenAI(
          base_url=f"{base_url}/v1",
          api_key="not-needed"  # 本地服务器不需要 API key
      )

      try:
          if stream:
              # 流式响应
              response = client.chat.completions.create(
                  model=model,
                  messages=[{"role": "user", "content": prompt}],
                  max_tokens=max_tokens,
                  temperature=temperature,
                  stream=True
              )

              print("响应:")
              for chunk in response:
                  if chunk.choices:
                      content = chunk.choices[0].delta.content
                      if content:
                          print(content, end="", flush=True)
              print("\n")
          else:
              # 非流式响应
              response = client.chat.completions.create(
                  model=model,
                  messages=[{"role": "user", "content": prompt}],
                  max_tokens=max_tokens,
                  temperature=temperature,
                  stream=False
              )

              content = response.choices[0].message.content
              print("响应:")
              print(content)
              print(f"\n使用 tokens: {response.usage.total_tokens}")

      except Exception as e:
          print(f"请求失败: {e}")

  def list_models(base_url: str):
      """列出可用模型"""
      try:
          client = OpenAI(
              base_url=f"{base_url}/v1",
              api_key="not-needed"
          )

          models = client.models.list()
          print("可用模型:")
          for model in models.data:
              print(f"- {model.id} (by {model.owned_by})")
      except Exception as e:
          print(f"请求失败: {e}")

  def main():
      parser = argparse.ArgumentParser(description="ERNIE GGUF OpenAI API 客户端")
      parser.add_argument("--url", default="http://localhost:8000", help="API 服务器 URL")
      parser.add_argument("--prompt", default="你好，请介绍一下自己。", help="要发送的提示文本")
      parser.add_argument("--model", default="ernie-gguf", help="模型名称")
      parser.add_argument("--max-tokens", type=int, default=1024, help="最大生成 token 数")
      parser.add_argument("--temperature", type=float, default=0.7, help="温度参数")
      parser.add_argument("--stream", action="store_true", help="启用流式响应")
      parser.add_argument("--list-models", action="store_true", help="列出可用模型")

      args = parser.parse_args()

      if args.list_models:
          list_models(args.url)
      else:
          chat_completion(
              base_url=args.url,
              prompt=args.prompt,
              model=args.model,
              max_tokens=args.max_tokens,
              temperature=args.temperature,
              stream=args.stream
          )

  if __name__ == "__main__":
      main()
  ```
