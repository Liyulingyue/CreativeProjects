# PaddleOCR-VL GGUF (llama.cpp 版)

## 项目概览

PaddleOCR-VL GGUF 项目将多模态模型拆分成「视觉编码器 + 语言模型」两部分,视觉侧保持 PyTorch,语言侧使用 GGUF 量化后通过 **llama.cpp** 系列工具直接加载,无需 Ollama 服务。

- 🎯 **目标**: 在消费级硬件上以最小的内存占用和延迟运行 PaddleOCR-VL
- 🧠 **视觉侧**: SiglipVisionModel + Projector (原生精度)
- 🗣️ **语言侧**: Ernie4.5 → GGUF 量化 → llama-cpp-python 推理
- 🔌 **接口**: 兼容 OpenAI Chat Completions API
- 🧰 **用途**: 图片 OCR + 对话、文档理解等多模态任务

## 关键特性

| 能力 | 说明 |
|------|------|
| 推理速度 | CPU 环境下可获得 2-5x 提升 |
| 架构解耦 | 视觉模块仍在 PyTorch 中运行,便于调试与扩展 |
| API 兼容 | 保持与 OpenAI 风格接口一致,可无缝集成现有应用 |
| 本地化 | 全流程离线部署,无外部服务依赖 |

## 项目结构

```
PaddleOCR-VL-GGUF/
├── demo_ppocrvl_gguf_server.py   # llama.cpp 后端服务器 (核心)
├── demo_ppocrvl_gguf_client.py   # 命令行客户端示例
├── convert_to_gguf.py            # 提取与导出 LLM 权重
├── demo_architecture.py          # 架构和参数统计脚本
├── requirements.txt              # 运行所需的 Python 依赖
├── README.md                     # 本文档 (整合版)
└── PaddlePaddle/
    └── PaddleOCR-VL/             # 官方 PaddleOCR-VL 权重 (需单独下载)
```

## 三步快速开始

### 1. 安装依赖

```bash
python -m venv .venv
source .venv/bin/activate  # Windows: .venv\Scripts\activate

pip install -r requirements.txt

# 安装 llama-cpp-python (CPU 版本)


# GPU/Metal 用户 (二选一)
# CUDA: CMAKE_ARGS="-DLLAMA_CUBLAS=on" pip install llama-cpp-python
# Metal: CMAKE_ARGS="-DLLAMA_METAL=on" pip install llama-cpp-python
```

### 2. 提取与量化语言模型

```bash
# 提取 Ernie4.5 相关权重 (保持在 PaddleOCR-VL-GGUF 目录下)
python convert_to_gguf.py \
    --model-path PaddlePaddle/PaddleOCR-VL \
    --output-path extracted_llm \
    --hf-output-dir extracted_llm/hf_model

# 使用 llama.cpp 将权重转换为 GGUF 并量化
# 安装必要的系统依赖 (Linux)
sudo apt update && sudo apt install -y libcurl4-openssl-dev

git clone https://github.com/ggml-org/llama.cpp
cd llama.cpp && cmake . && cmake --build . -j$(nproc) && cd ..

# 使用虚拟环境中的 Python 运行转换脚本
python llama.cpp/convert_hf_to_gguf.py \
  extracted_llm/hf_model \
  --outfile extracted_llm/llm_model.gguf \
  --outtype f16

# 使用编译后的二进制进行量化
./llama.cpp/bin/llama-quantize extracted_llm/llm_model.gguf \
                              extracted_llm/llm_model_q4.gguf Q4_K_M
```

### 3. 启动服务并测试

```bash
# 终端 1: 启动多模态服务
# in PaddleOCR-VL-GGUF
python demo_ppocrvl_gguf_server.py

# 终端 2: 发送测试请求
python demo_ppocrvl_gguf_client.py \
    --image /path/to/image.jpg
```

## 其他说明

### 提取语言模型权重

`convert_to_gguf.py` 会在 `extracted_llm/` 目录下生成以下文件:

- `llm_model.pt` / `lm_head.pt`: PyTorch 权重
- `llm_config.json`: 配置文件,供后续转换脚本使用
- `hf_model/`: 可直接给 `convert_hf_to_gguf.py` 使用的 Hugging Face 检查点 (如需关闭可添加 `--no-hf-export`)

### 使用 llama.cpp 转换与量化

1. 克隆并编译 llama.cpp (可选 GPU 支持)
2. 编写/适配 `convert.py` 将提取的权重转为 GGUF
3. 通过 `quantize` 进行量化,推荐 `Q4_K_M`

常用命令:

```bash
./quantize input.gguf output_q4.gguf Q4_K_M
./quantize input.gguf output_q5.gguf Q5_K_M
```

量化等级建议:

| 等级 | 内存 | 质量 | 备注 |
|------|------|------|------|
| Q4_0 | 最低 | 较低 | 调试与原型 |
| **Q4_K_M** | 低 | 佳 | 默认推荐 |
| Q5_K_M | 中 | 很好 | 质量优先 |
| Q8_0 | 高 | 接近 FP16 | 高精度需求 |

### 配置服务器参数

`demo_ppocrvl_gguf_server.py` 中的关键参数:

```python
GGUF_MODEL_PATH = "extracted_llm/llm_model_q4.gguf"  # GGUF 模型路径
N_GPU_LAYERS = 0     # GPU 层数 (0=纯 CPU, 适当增大可用 GPU 加速)
N_CTX = 4096         # 上下文窗口
N_THREADS = 8        # CPU 线程数,建议与物理核心数匹配
```

GPU 用户可根据显存设置 `N_GPU_LAYERS` (例如 32 或更高)。

### API 调用示例

```python
import requests

payload = {
    "messages": [
        {
            "role": "user",
            "content": [
                {"type": "text", "text": "识别图片中的文字"},
                {"type": "image_url", "image_url": {"url": "data:image/jpeg;base64,..."}}
            ]
        }
    ],
    "max_tokens": 1024,
    "temperature": 0.7,
    "stream": False
}

response = requests.post(
    "http://localhost:7778/v1/chat/completions",
    json=payload,
    timeout=120
)

print(response.json()["choices"][0]["message"]["content"])
```

## 架构说明

```
┌─────────────────────────────────────────────────────────────┐
│                 PaddleOCR-VL GGUF 混合架构                   │
├─────────────────────────────────────────────────────────────┤
│ 输入图像 + 文本提示                                         │
│    │                                                        │
│    ▼                                                        │
│ 视觉编码器 (PyTorch)                                        │
│  ├─ SiglipVisionModel       ≈200M 参数                      │
│  └─ Attention Pooling                                        │
│    │                                                        │
│    ▼                                                        │
│ Projector (PyTorch)       ≈20M 参数                          │
│    │                                                        │
│    ▼ 视觉嵌入                                               │
├─────────────────────────────────────────────────────────────┤
│ llama-cpp 推理 (GGUF)                                       │
│  ├─ Ernie4.5 Decoder       量化后 ≈500-800MB               │
│  └─ LM Head                量化后 ≈100MB                   │
│    │                                                        │
│    ▼                                                        │
│ 生成文本输出                                                │
└─────────────────────────────────────────────────────────────┘
```

> 视觉部分仍保持原始精度,主要的性能与内存优化集中在 LLM 侧。

## 贡献与许可证

- 🚀 欢迎通过 Issue/PR 提交改进建议或补充转换脚本
- 📄 许可证遵循 PaddleOCR-VL 原项目,详情参见仓库根目录 `LICENSE`

## 参考资源

- [PaddleOCR 官方仓库](https://github.com/PaddlePaddle/PaddleOCR)
- [llama.cpp](https://github.com/ggml-org/llama.cpp)
- [llama-cpp-python](https://github.com/abetlen/llama-cpp-python)
- [GGUF 格式说明](https://github.com/ggerganov/ggml/blob/master/docs/gguf.md)

如遇问题,请在 GitHub Issues 中反馈或提交讨论。


## 运行耗时测试
|设备 | 图片尺寸 | 耗时(秒) |
|----|---------|---------|
| RDK X5(8x A55@1.5GHz, 4G内存版本) | 256×256 | 45 |
| RDK X5(8x A55@1.5GHz, 4G内存版本) | 640x480 | 97.06 |
| Intel Ultra5 | 256×256 | 4.55 |
| Intel Ultra5 | 640x480 | 8.59 |