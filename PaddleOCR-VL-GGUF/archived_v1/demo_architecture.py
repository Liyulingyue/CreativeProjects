#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
PaddleOCR-VL GGUF 版本演示脚本
展示如何分离视觉编码器和 LLM 部分
"""

import torch
import sys
from pathlib import Path

def demo_vision_extraction():
    """
    演示: 提取视觉编码器和投影层
    """
    print("=" * 60)
    print("演示 1: 提取视觉编码器")
    print("=" * 60)
    
    from transformers import AutoModelForCausalLM
    
    model_path = "PaddlePaddle/PaddleOCR-VL"
    
    if not Path(model_path).exists():
        print(f"错误: 模型路径不存在: {model_path}")
        print("请先下载 PaddleOCR-VL 模型")
        return
    
    print(f"加载模型: {model_path}")
    model = AutoModelForCausalLM.from_pretrained(
        model_path,
        trust_remote_code=True,
        torch_dtype=torch.float32,
        low_cpu_mem_usage=True
    )
    
    print("\n模型组件:")
    print(f"1. 视觉编码器: {type(model.visual)}")
    print(f"2. 投影层: {type(model.mlp_AR)}")
    print(f"3. LLM 模型: {type(model.model)}")
    print(f"4. LM Head: {type(model.lm_head)}")
    
    # 统计参数量
    def count_parameters(module):
        return sum(p.numel() for p in module.parameters())
    
    vision_params = count_parameters(model.visual)
    projector_params = count_parameters(model.mlp_AR)
    llm_params = count_parameters(model.model)
    lm_head_params = count_parameters(model.lm_head)
    
    total_params = vision_params + projector_params + llm_params + lm_head_params
    
    print("\n参数统计:")
    print(f"  视觉编码器: {vision_params:,} ({vision_params/1e6:.2f}M) - {vision_params/total_params*100:.1f}%")
    print(f"  投影层:     {projector_params:,} ({projector_params/1e6:.2f}M) - {projector_params/total_params*100:.1f}%")
    print(f"  LLM 模型:   {llm_params:,} ({llm_params/1e6:.2f}M) - {llm_params/total_params*100:.1f}%")
    print(f"  LM Head:    {lm_head_params:,} ({lm_head_params/1e6:.2f}M) - {lm_head_params/total_params*100:.1f}%")
    print(f"  总计:       {total_params:,} ({total_params/1e6:.2f}M)")
    
    print("\n✓ 可以看到 LLM 部分占据了主要参数量")
    print("✓ 通过 GGUF 量化 LLM 部分可以显著减少内存占用")
    
    return model


def demo_architecture():
    """
    演示: 分析模型架构
    """
    print("\n" + "=" * 60)
    print("演示 2: 模型架构分析")
    print("=" * 60)
    
    print("\nPaddleOCR-VL 是一个两阶段的多模态模型:\n")
    
    print("阶段 1: 视觉处理 (非 LLM 部分)")
    print("  ├─ SiglipVisionEmbeddings: 图像 patch 嵌入")
    print("  ├─ SiglipEncoder: 视觉 Transformer")
    print("  ├─ SiglipMultiheadAttentionPoolingHead: 注意力池化")
    print("  └─ Projector: 将视觉特征投影到 LLM 空间")
    
    print("\n阶段 2: 语言生成 (LLM 部分 - 可用 GGUF)")
    print("  ├─ Ernie4_5Model:")
    print("  │   ├─ embed_tokens: 词嵌入")
    print("  │   ├─ layers: Transformer 层")
    print("  │   │   ├─ self_attn: 自注意力")
    print("  │   │   └─ mlp: 前馈网络")
    print("  │   └─ norm: 最终归一化")
    print("  └─ lm_head: 输出投影到词表")
    
    print("\n分离策略:")
    print("  ✓ 视觉部分: 保持 PyTorch (确保兼容性)")
    print("  ✓ LLM 部分: 转换为 GGUF (加速推理)")
    print("  ✓ 中间层: 通过嵌入传递连接两部分")


def demo_quantization_benefits():
    """
    演示: 量化收益估算
    """
    print("\n" + "=" * 60)
    print("演示 3: 量化收益估算")
    print("=" * 60)
    
    # 假设 LLM 部分有 1B 参数
    llm_params = 900_000_000  # 约 900M 参数
    
    print(f"\n假设 LLM 部分参数量: {llm_params/1e9:.1f}B")
    
    formats = {
        "FP32": (32, "原始精度"),
        "FP16": (16, "半精度"),
        "INT8": (8, "8-bit 量化"),
        "Q4_K_M": (4.5, "GGUF 混合 4-bit"),
        "Q4_0": (4, "GGUF 纯 4-bit"),
    }
    
    print("\n不同格式的内存占用:")
    print(f"{'格式':<10} {'位数':>6} {'大小 (GB)':>12} {'说明':<20}")
    print("-" * 60)
    
    for format_name, (bits, desc) in formats.items():
        size_gb = (llm_params * bits) / (8 * 1024**3)
        print(f"{format_name:<10} {bits:>6.1f} {size_gb:>12.2f} {desc:<20}")
    
    fp32_size = (llm_params * 32) / (8 * 1024**3)
    q4_size = (llm_params * 4.5) / (8 * 1024**3)
    savings = (1 - q4_size / fp32_size) * 100
    
    print(f"\n使用 Q4_K_M 量化可节省内存: {savings:.1f}%")
    print(f"推理速度提升: 约 2-3 倍")
    print(f"精度损失: 轻微 (大多数场景可接受)")


def demo_workflow():
    """
    演示: 完整工作流程
    """
    print("\n" + "=" * 60)
    print("演示 4: GGUF 工作流程")
    print("=" * 60)
    
    print("\n步骤 1: 提取 LLM 权重")
    print("  $ python convert_to_gguf.py \\")
    print("      --model-path PaddlePaddle/PaddleOCR-VL \\")
    print("      --output-path ./extracted_llm")
    
    print("\n步骤 2: 转换为 GGUF 格式")
    print("  $ cd llama.cpp")
    print("  $ python convert.py ../extracted_llm \\")
    print("      --outfile ../extracted_llm/llm_model.gguf")
    
    print("\n步骤 3: 量化模型")
    print("  $ ./quantize \\")
    print("      ../extracted_llm/llm_model.gguf \\")
    print("      ../extracted_llm/llm_model_q4.gguf \\")
    print("      Q4_K_M")
    
    print("\n步骤 4: 导入 Ollama")
    print("  $ cd ../extracted_llm")
    print("  $ ollama create paddleocr-vl-llm -f Modelfile")
    
    print("\n步骤 5: 启动服务")
    print("  $ python demo_ppocrvl_gguf_server.py")
    
    print("\n步骤 6: 测试")
    print("  $ python demo_ppocrvl_gguf_client.py \\")
    print("      --text '识别文字' \\")
    print("      --image image.jpg")


def main():
    print("\n" + "=" * 60)
    print("PaddleOCR-VL GGUF 版本演示")
    print("=" * 60)
    
    try:
        # 演示 1: 提取视觉编码器
        model = demo_vision_extraction()
        
        # 演示 2: 架构分析
        demo_architecture()
        
        # 演示 3: 量化收益
        demo_quantization_benefits()
        
        # 演示 4: 工作流程
        demo_workflow()
        
        print("\n" + "=" * 60)
        print("演示完成!")
        print("=" * 60)
        print("\n下一步:")
        print("1. 运行 convert_to_gguf.py 提取 LLM 权重")
        print("2. 参考 README.md 完成 GGUF 转换")
        print("3. 启动 demo_ppocrvl_gguf_server.py 测试")
        
    except Exception as e:
        print(f"\n错误: {e}")
        import traceback
        traceback.print_exc()
        sys.exit(1)


if __name__ == "__main__":
    main()
