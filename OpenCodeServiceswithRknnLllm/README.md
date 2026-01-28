# Qwen2.5-Coder + OpenCode Server on RK3588S

本项目旨在 Rockchip RK3588S (CoolPi 4B) 硬件上部署 Qwen2.5-1.5B-Coder 模型，并将其作为 OpenCode 的后端服务端，提供本地化的 AI 编程助手服务。

## 目录结构
- `opencode/`: OpenCode 源码，用于构建和配置代理/服务端。
- `rknn-llm/`: Rockchip 官方提供的 RKNN-LLM 工具链及运行时环境。

---

## 快速指南

### 步骤 1: 获取项目源码

首先，克隆本项目及其子模块（包含 OpenCode 和 RKNN-LLM）：

```bash
# 克隆主项目
git clone https://github.com/Liyulingyue/CreativeProjects.git
cd CreativeProjects/OpenCodeServiceswithRknnLllm

# 如果子模块未初始化，可以手动克隆相关仓库
git clone https://github.com/anomalyco/opencode.git
git clone https://github.com/airockchip/rknn-llm.git
```

### 步骤 2: 环境准备

#### 1.1 RKNN-LLM Toolkit (PC 端 - 模型转换)
在您的 PC (x86_64 Linux) 上执行：
```bash
# 进入 toolkit 目录
cd rknn-llm/rkllm-toolkit/packages/

# 创建并进入虚拟环境 (推荐)
conda create -n rkllm python=3.10
conda activate rkllm

# 安装依赖
pip install -r requirements.txt

# 安装对应 python 版本的 toolkit
pip install rkllm_toolkit-1.2.3-cp310-cp310-linux_x86_64.whl
```

#### 1.2 RKNN-LLM Runtime (板端 - RK3588S)
在您的 CoolPi 4B (RK3588S) 上执行：
```bash
# 1. 更新 NPU 驱动 (如果版本过低)
cd rknn-llm/rknpu-driver/
# 解压并根据内部 README.md 指引起驱动更新

# 2. 配置运行时库
sudo cp rknn-llm/rkllm-runtime/Linux/librkllm_api/aarch64/librkllmrt.so /usr/lib/
```

### 步骤 3: 模型转换 (HuggingFace -> RKLLM)

在 PC 端编写 `convert.py`:
```python
from rkllm.api import RKLLM

rkllm = RKLLM()
# 1. 加载模型 (请替换为 Qwen2.5-Coder-1.5B 的实际路径)
rkllm.load_huggingface(model='/path/to/Qwen2.5-Coder-1.5B', model_type='qwen')

# 2. 构建模型 (针对 RK3588 优化并量化)
rkllm.build(do_quantization=True, optimization_level=1, device='rk3588')

# 3. 导出模型
rkllm.export_rkllm(export_path='./qwen2_5_coder_1_5b.rkllm')
```
运行：`python convert.py`，并将生成的 `.rkllm` 文件拷贝到板端。

### 步骤 4: 部署服务端

官方提供的 Flask Demo 已经兼容 OpenAI 的响应格式，只需启动即可：

1. **安装依赖** (在 RK3588S 上):
   ```bash
   pip install fastapi uvicorn requests flask
   ```

2. **启动 Server**:
   进入 `rknn-llm/examples/rkllm_server_demo/`，执行：
   ```bash
   # 赋予脚本执行权限
   chmod +x build_rkllm_server_flask.sh
   
   # 启动服务 (根据实际路径替换模型路径)
   ./build_rkllm_server_flask.sh --workshop ./ --model_path /home/liyulingyue/models/qwen2_5_coder_1_5b.rkllm --platform rk3588
   ```
   Server 默认运行在 `8080` 端口，API 路径为 `http://<rk3588_ip>:8080/rkllm_chat`。

### 步骤 5: 配置 OpenCode

由于 `opencode` 默认可能寻找 `/v1/chat/completions`，您可以使用 Nginx 进行路径转发，或者直接在配置中尝试指定路径（如果支持）：

在项目根目录下创建 `opencode.jsonc`：

```jsonc
{
  "$schema": "https://opencode.ai/config.json",
  "provider": {
    "openai": {
      "options": {
        // 如果 opencode 支持完整 URL，请直接填写官方路径
        // 否则请使用轻量级代理将 /v1/chat/completions 转发至 /rkllm_chat
        "baseURL": "http://127.0.0.1:8080/", 
        "apiKey": "local-rk3588"
      }
    }
  }
}
```

> **提示**：如果 OpenCode 强制要求 `/v1` 前缀，可以使用一个简单的 Python 脚本（类似 `ErnieToolCallsProxy`）做一层路由转发。

### 步骤 6: 编译与运行 OpenCode

如果您使用的是源码，需先安装 Bun 环境，然后执行：

```bash
cd opencode
bun install
bun run dev
```

---

## 常用工具与文档
- **RKNN-LLM 文档**: [rknn-llm/doc/](rknn-llm/doc/)
- **OpenCode 文档**: [opencode/README.md](opencode/README.md)
- **CPU 查看**: `lscpu` (您当前设备已确认是 RK3588S)
