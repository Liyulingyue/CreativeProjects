# RK3588 (CoolPi 4B) 模型部署全指南：LLM 与 VLLM

本项目涵盖了在 Rockchip RK3588 平台上部署 **纯文本大模型 (LLM)** 和 **多模态视觉模型 (VLLM)** 的完整流程。

---

## 1. 核心概念与后缀区分

在 RKNN 生态中，后缀决定了模型运行的引擎：

| 后缀 | 说明 | 运行引擎 | 典型模型 |
| :--- | :--- | :--- | :--- |
| **`.rkllm`** | **文本大模型** | RKLLM-Runtime (`librkllmrt.so`) | Qwen2.5-Coder, Llama3 |
| **`.rknn`** | **通用 NPU 模型** | RKNN-Runtime (`librknnrt.so`) | YOLO, Qwen-VL 的视觉部分 |

> **⚠️ 常见错误**：尝试用 `flask_server.py` 加载 `.rknn` 文件会导致 `invalid rkllm model!` 报错。

---

## 2. 模式一：纯 LLM 部署 (以 Qwen2.5-Coder 为例)

适用于提供 OpenAI 兼容接口的编程助手。

### 环境安装 (板端)
```bash
pip install flask==2.2.2 Werkzeug==2.2.2
```

### 启动服务 (原生运行)
进入服务端目录，这很重要，因为代码通过相对路径加载 `lib/` 下的动态库：
```bash
cd rknn-llm/examples/rkllm_server_demo/rkllm_server/

python3 flask_server.py \
  --rkllm_model_path /path/to/your/qwen2_5_coder_1_5b.rkllm \
  --target_platform rk3588
```

### 测试接口 (使用 curl)
服务启动后，你可以通过 `curl` 快速验证对话功能：
```bash
curl http://localhost:8080/rkllm_chat \
  -H "Content-Type: application/json" \
  -d '{
    "messages": [
      {"role": "user", "content": "你好，请写一段 python 冒泡排序。"}
    ],
    "stream": false
  }'
```
*   **API 接口**：`http://[板端IP]:8080/rkllm_chat`
*   **说明**：该脚本已针对 OpenAI 格式做了兼容，可直接对接 OpenCode。

---

## 3. 模式二：多模态 VLLM 部署 (以 Qwen3-VL 为例)

多模态模型被拆分为“视觉”与“文本”两个零件。目前在板端，**官方推荐使用 C++ 进行高性能部署**。

### 部署原理
1.  **Vision 部件 (.rknn)**：提取图像 Embedding。
2.  **LLM 部件 (.rkllm)**：接收文本 + 图像 Embedding 生成回复。

### 编译与运行 (板端)
```bash
# 1. 进入编译目录
cd rknn-llm/examples/multimodal_model_demo/deploy/

# 2. 执行编译脚本
./build-linux.sh

# 3. 运行 demo
cd install/multimodal_model_demo_Linux/
./demo image.jpg \
       ./vision_model.rknn \
       ./llm_model.rkllm \
       2048 4096 3 \
       "<|vision_start|>" "<|vision_end|>" "<|image_pad|>"
```

---

## 4. 深度讨论：为什么 VLLM 只有 C++ Demo？

### Q1: 可以在 Python 中运行 VLLM 吗？
**可以，但需要手动“缝合”。** 
目前官方没有提供 Python 版的端到端 VLLM 库。你需要在 Python 中调用 `rknn-toolkit2` 跑视觉推理得到特征，再通过 `rkllm` 的 `RKLLM_INPUT_EMBED` 接口传给语言模型。这涉及大量的内存拷贝和底层数据对齐，性能较差。

### Q2: DDR 频率相关的报错重要吗？
在运行调试脚本时（如 `fix_freq_rk3588.sh`），可能会看到 `dmc/available_frequencies` 找不到。
*   **原因**：不同发行版（如 CoolPi 的 Ubuntu）NPU/DDR 的 sysfs 路径可能与官方标准不同。
*   **影响**：这仅影响“锁定最高频率”功能。模型依然可以运行，只是可能由于动态调频导致初次推理略有延迟。

### Q3: 报错 `invalid rkllm model!` 怎么查？
1.  **格式对吗？** 必须是 `.rkllm`，不能是 `.rknn`。
2.  **平台对吗？** 转换模型时 `device` 参数必须指定为 `rk3588`。
3.  **驱动版本？** 检查日志中的 `rknpu driver version` 是否过低。

---

## 5. 建议工作流

1.  **AI 编程助手**：使用 **Qwen2.5-Coder-1.5B (RKLLM)** + **Flask Server**。
2.  **视觉理解**：使用 **Qwen3-VL (RKNN + RKLLM)** + **C++ Native Demo**。
3.  **集成方案**：如果需要在 Python 中使用 VLLM，可以考虑用 C++ 编写一个简单的 Web Server，或使用 `ctypes` 桥接。
