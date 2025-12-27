# llama-cpp-ernie-tutorial

本教程演示如何将 ERNIE 4.5 模型转换为 GGUF 格式，并使用 llama.cpp 启动兼容 OpenAI API 的服务。

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

### 安装 CMake

1. **使用 winget（推荐，Windows 10+）：**
   ```powershell
   winget install Kitware.CMake
   ```

2. 验证安装：打开命令提示符或 PowerShell，运行 `cmake --version` 确认安装成功。

安装完成后，请重新打开终端以确保 PATH 更新。

### 安装 MinGW (Windows)

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
```

## 方式二：使用预编译文件和下载 GGUF 模型

如果你不想从源码编译，可以下载预编译的 llama.cpp 和已经转换好的 GGUF 模型。

### 下载预编译的 llama.cpp

从 [llama.cpp releases](https://github.com/ggml-org/llama.cpp/releases) 下载最新版本的预编译二进制文件。根据你的操作系统选择合适的版本，解压到项目目录。

例如，下载 `llama-b7554-bin-win-cpu-x64` 并解压到。

### 下载转换好的 GGUF 模型

从 Hugging Face 或 ModelScope 下载已经转换好的 ERNIE GGUF 模型：

- Hugging Face: 搜索 "ERNIE GGUF"
- ModelScope: 搜索 "ERNIE GGUF"

将下载的 `.gguf` 文件放置在项目根目录，并命名为 `ernie.gguf`。

## 启动服务和测试

> **注意：** 如果你创建了虚拟环境，请确保先激活它：
> - Windows (PowerShell)：`.\.venv\Scripts\Activate.ps1`
> - Windows (命令提示符)：`.\.venv\Scripts\activate.bat`
> - Linux/macOS：`source .venv/bin/activate`

### 1. 启动服务

使用 llama.cpp 的服务器：

```bash
./llama.cpp/build/bin/server --model ./ernie.gguf --host 0.0.0.0 --port 8000 --ctx-size 4096
```

或者使用我们提供的 server.py 脚本（推荐），它封装了默认参数配置，无需每次都输入长命令：

```bash
# 
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

或者使用 curl：

```bash
curl -X POST http://localhost:8000/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{
    "model": "ernie-gguf",
    "messages": [{"role": "user", "content": "你好"}]
  }'
```

## 文件说明

- `convert_to_gguf.py`: 模型转换脚本 (调用 llama.cpp convert)
- `server.py`: 启动 llama.cpp 服务器的脚本
- `client_request.py`: 使用 requests 库的测试客户端
- `client_openai.py`: 使用 OpenAI 库的测试客户端
- `requirements.txt`: Python 依赖
