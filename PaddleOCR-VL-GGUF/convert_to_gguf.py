#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
将 PaddleOCR-VL 的 LLM 部分（Ernie4_5Model）提取并转换为 GGUF 格式
以便使用 ollama 进行加速推理
"""

import os
import sys
import torch
import argparse
from pathlib import Path
from transformers import AutoModelForCausalLM, AutoConfig
import json
import struct

def extract_llm_weights(model_path, output_path):
    """
    从完整的 PaddleOCR-VL 模型中提取 LLM 部分的权重
    
    Args:
        model_path: PaddleOCR-VL 模型路径
        output_path: 输出目录
    """
    print(f"正在加载完整模型: {model_path}")
    
    # 加载完整模型
    full_model = AutoModelForCausalLM.from_pretrained(
        model_path,
        trust_remote_code=True,
        torch_dtype=torch.float32,
        low_cpu_mem_usage=True
    )
    
    config = AutoConfig.from_pretrained(model_path, trust_remote_code=True)
    
    print("模型加载完成")
    print(f"模型架构: {type(full_model)}")
    print(f"LLM 部分: {type(full_model.model)}")
    
    # 提取 LLM 部分
    llm_model = full_model.model
    lm_head = full_model.lm_head
    
    # 保存为 PyTorch 格式
    output_dir = Path(output_path)
    output_dir.mkdir(parents=True, exist_ok=True)
    
    llm_weights_path = output_dir / "llm_model.pt"
    lm_head_path = output_dir / "lm_head.pt"
    config_path = output_dir / "llm_config.json"
    
    print(f"保存 LLM 权重到: {llm_weights_path}")
    torch.save(llm_model.state_dict(), llm_weights_path)
    
    print(f"保存 LM Head 到: {lm_head_path}")
    torch.save(lm_head.state_dict(), lm_head_path)
    
    # 保存配置
    llm_config = {
        "vocab_size": config.vocab_size,
        "hidden_size": config.hidden_size,
        "intermediate_size": config.intermediate_size,
        "num_hidden_layers": config.num_hidden_layers,
        "num_attention_heads": config.num_attention_heads,
        "num_key_value_heads": config.num_key_value_heads,
        "max_position_embeddings": config.max_position_embeddings,
        "rms_norm_eps": config.rms_norm_eps,
        "rope_theta": config.rope_theta,
        "use_bias": config.use_bias,
        "hidden_act": config.hidden_act,
        "head_dim": config.head_dim,
        "rope_scaling": config.rope_scaling,
    }
    
    print(f"保存配置到: {config_path}")
    with open(config_path, 'w', encoding='utf-8') as f:
        json.dump(llm_config, f, indent=2, ensure_ascii=False)
    
    print("\n=== 提取的权重信息 ===")
    total_params = 0
    for name, param in llm_model.named_parameters():
        num_params = param.numel()
        total_params += num_params
        print(f"{name}: {param.shape} ({num_params:,} 参数)")
    
    print(f"\n总参数量: {total_params:,} ({total_params / 1e6:.2f}M)")
    
    print("\n=== 提取完成 ===")
    print(f"提取的文件:")
    print(f"  - {llm_weights_path}")
    print(f"  - {lm_head_path}")
    print(f"  - {config_path}")
    
    return output_dir


def create_conversion_guide(llm_config_path, output_path):
    """
    创建 GGUF 转换指南
    """
    with open(llm_config_path, 'r', encoding='utf-8') as f:
        config = json.load(f)
    
    readme_path = Path(output_path) / "CONVERSION_README.md"
    readme_content = f"""# PaddleOCR-VL LLM 转 GGUF 指南

## 已提取的文件

- `llm_model.pt`: LLM 部分的 PyTorch 权重
- `lm_head.pt`: Language Model Head 权重  
- `llm_config.json`: 模型配置

## 转换为 GGUF 步骤

### 1. 安装 llama.cpp

```bash
git clone https://github.com/ggerganov/llama.cpp
cd llama.cpp
make
```

### 2. 准备模型权重

需要将 PyTorch 权重转换为 llama.cpp 可识别的格式。由于 Ernie4.5 基于类似 Llama 的架构，可以参考以下步骤:

```bash
# 将 PyTorch 权重转换为 HuggingFace 格式（如果需要）
python convert_to_hf.py --input llm_model.pt --output ./hf_model

# 使用 llama.cpp 转换工具
python llama.cpp/convert.py ./hf_model --outfile llm_model.gguf --outtype f16
```

### 3. 量化模型（推荐，大幅加速）

```bash
# 量化为 Q4_K_M 格式（推荐）
./llama.cpp/quantize llm_model.gguf llm_model_q4.gguf Q4_K_M

# 或其他量化级别:
# - Q4_0: 最快，质量较低
# - Q4_K_M: 平衡（推荐）
# - Q5_K_M: 更高质量
# - Q8_0: 接近原始精度
```

### 4. 使用 llama-cpp-python 加载

```python
from llama_cpp import Llama

# 加载 GGUF 模型
llm = Llama(
    model_path="llm_model_q4.gguf",
    n_gpu_layers=0,  # 0 表示纯 CPU，>0 使用 GPU
    n_ctx=4096,      # 上下文长度
    n_threads=8      # CPU 线程数
)

# 生成文本
output = llm(
    "你好",
    max_tokens=100,
    temperature=0.7
)
print(output['choices'][0]['text'])
```

### 5. 配置 GGUF 服务器

编辑 `demo_ppocrvl_gguf_server.py` 中的配置:

```python
GGUF_MODEL_PATH = "extracted_llm/llm_model_q4.gguf"  # GGUF 模型路径
N_GPU_LAYERS = 0  # GPU 层数，0 表示纯 CPU
N_CTX = 4096      # 上下文长度
N_THREADS = 8     # CPU 线程数
```

然后启动服务器:

```bash
python demo_ppocrvl_gguf_server.py
```

## 模型配置

- **词汇表大小**: {config['vocab_size']}
- **隐藏层大小**: {config['hidden_size']}
- **层数**: {config['num_hidden_layers']}
- **注意力头数**: {config['num_attention_heads']}
- **最大上下文长度**: {config['max_position_embeddings']}

## 量化级别对比

| 级别 | 每参数位数 | 模型大小 | 速度 | 质量 | 推荐场景 |
|------|-----------|---------|------|------|---------|
| Q4_0 | 4.0 | 最小 | 最快 | 较低 | 快速原型 |
| Q4_K_M | ~4.5 | 小 | 快 | 良好 | **生产推荐** |
| Q5_K_M | ~5.5 | 中等 | 中等 | 很好 | 质量优先 |
| Q8_0 | 8.0 | 较大 | 较慢 | 接近原始 | 高精度需求 |

## 注意事项

1. **权重映射**: Ernie4.5 的权重需要映射到 llama.cpp 的格式，可能需要自定义转换脚本
2. **RoPE 配置**: 注意 rope_scaling 参数的转换（Ernie4.5 使用 MRoPE）
3. **特殊 Token**: 确保 tokenizer 的特殊 token 正确配置

## GPU 加速

### CUDA (NVIDIA GPU)

```bash
# 编译支持 CUDA 的 llama.cpp
cd llama.cpp
make clean
LLAMA_CUBLAS=1 make

# 或安装支持 CUDA 的 llama-cpp-python
CMAKE_ARGS="-DLLAMA_CUBLAS=on" pip install llama-cpp-python --force-reinstall --no-cache-dir

# 在代码中启用 GPU
llm = Llama(model_path="...", n_gpu_layers=32)  # 使用 GPU
```

### Metal (Mac M1/M2)

```bash
# 编译支持 Metal 的 llama.cpp
cd llama.cpp
make clean
LLAMA_METAL=1 make

# 或安装支持 Metal 的 llama-cpp-python
CMAKE_ARGS="-DLLAMA_METAL=on" pip install llama-cpp-python --force-reinstall --no-cache-dir
```

## 替代方案

如果 GGUF 转换遇到困难，可以考虑:

1. **使用 vLLM**: 支持多种模型格式，性能优秀
2. **使用 TensorRT-LLM**: NVIDIA GPU 上性能最佳
3. **使用 ExLlamaV2**: 支持 GPTQ 量化的高效推理

## 相关资源

- llama.cpp: https://github.com/ggerganov/llama.cpp
- llama-cpp-python: https://github.com/abetlen/llama-cpp-python
- GGUF 格式说明: https://github.com/ggerganov/ggml/blob/master/docs/gguf.md
"""
    
    with open(readme_path, 'w', encoding='utf-8') as f:
        f.write(readme_content)
    
    print(f"转换指南已创建: {readme_path}")


def main():
    parser = argparse.ArgumentParser(description="提取 PaddleOCR-VL 的 LLM 部分并准备 GGUF 转换")
    parser.add_argument(
        "--model-path",
        type=str,
        default="PaddlePaddle/PaddleOCR-VL",
        help="PaddleOCR-VL 模型路径"
    )
    parser.add_argument(
        "--output-path",
        type=str,
        default="./extracted_llm",
        help="输出目录"
    )
    
    args = parser.parse_args()
    
    print("=" * 60)
    print("PaddleOCR-VL LLM 提取工具")
    print("=" * 60)
    
    # 提取权重
    output_dir = extract_llm_weights(args.model_path, args.output_path)
    
    # 创建转换指南
    config_path = output_dir / "llm_config.json"
    create_conversion_guide(config_path, output_dir)
    
    print("\n" + "=" * 60)
    print("提取完成!")
    print("=" * 60)
    print(f"\n请查看 {output_dir}/CONVERSION_README.md 了解后续转换步骤")


if __name__ == "__main__":
    main()
