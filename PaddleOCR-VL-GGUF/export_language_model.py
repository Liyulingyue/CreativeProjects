#!/usr/bin/env python3
"""
导出 PaddleOCR-VL 语言模型部分
提取 LLM 权重并准备 GGUF 转换所需的格式
"""

import argparse
import copy
import json
from pathlib import Path
import torch
from transformers import AutoConfig, AutoModelForCausalLM, AutoTokenizer

def sanitize_text_config(base_config: AutoConfig) -> AutoConfig:
    """Create a text-only config compatible with Ernie4.5 causal LM."""
    text_config = copy.deepcopy(base_config)
    text_config.architectures = ["Ernie4_5ForCausalLM"]
    text_config.model_type = "ernie4_5"
    text_config.is_encoder_decoder = False
    text_config.add_cross_attention = False
    text_config.tie_encoder_decoder = False

    if getattr(text_config, "num_key_value_heads", None) is None:
        text_config.num_key_value_heads = text_config.num_attention_heads

    text_config.auto_map = {
        "AutoConfig": "configuration_paddleocr_vl.PaddleOCRVLConfig",
        "AutoModelForCausalLM": "modeling_paddleocr_vl.Ernie4_5ForCausalLM",
    }

    return text_config

def export_language_model(input_path: str, output_path: str, create_hf_checkpoint: bool = True):
    """
    导出语言模型部分

    Args:
        input_path: 输入的完整模型路径
        output_path: 输出路径
        create_hf_checkpoint: 是否创建HuggingFace格式的检查点
    """
    print(f"正在加载完整模型: {input_path}")
    full_model = AutoModelForCausalLM.from_pretrained(
        input_path,
        trust_remote_code=True,
        torch_dtype=torch.float32,
        low_cpu_mem_usage=True
    )

    base_config = AutoConfig.from_pretrained(input_path, trust_remote_code=True)
    text_config = sanitize_text_config(base_config)

    print("模型加载完成")
    print(f"模型架构: {type(full_model)}")
    print(f"LLM 部分: {type(full_model.model)}")

    # 提取 LLM 部分
    llm_model = full_model.model
    lm_head = full_model.lm_head

    # 创建输出目录
    output_dir = Path(output_path)
    output_dir.mkdir(parents=True, exist_ok=True)

    # 保存为 PyTorch 格式
    llm_weights_path = output_dir / "llm_model.pt"
    lm_head_path = output_dir / "lm_head.pt"
    config_path = output_dir / "llm_config.json"

    print(f"保存 LLM 权重到: {llm_weights_path}")
    llm_state = llm_model.state_dict()
    torch.save(llm_state, llm_weights_path)

    print(f"保存 LM Head 到: {lm_head_path}")
    lm_head_state = lm_head.state_dict()
    torch.save(lm_head_state, lm_head_path)

    # 保存配置
    print(f"保存配置到: {config_path}")
    with open(config_path, 'w', encoding='utf-8') as f:
        json.dump(text_config.to_dict(), f, indent=2, ensure_ascii=False)

    # 创建 HuggingFace 格式检查点（用于 GGUF 转换）
    if create_hf_checkpoint:
        hf_dir = output_dir / "hf_model"
        hf_dir.mkdir(exist_ok=True)

        print(f"创建 HuggingFace 格式检查点: {hf_dir}")

        # 使用 from_config 创建模型，避免配置不匹配的问题
        try:
            ernie_model = AutoModelForCausalLM.from_config(text_config, trust_remote_code=True)
        except Exception as e:
            print(f"警告: from_config 失败，使用备用方法: {e}")
            # 备用方法：创建标准 Ernie 模型并手动加载权重
            from transformers import ErnieForCausalLM
            ernie_config = copy.deepcopy(text_config)
            ernie_config.model_type = "ernie"
            ernie_config.architectures = ["ErnieForCausalLM"]
            # 添加缺失的配置项
            if not hasattr(ernie_config, 'type_vocab_size'):
                ernie_config.type_vocab_size = 2
            if not hasattr(ernie_config, 'max_position_embeddings'):
                ernie_config.max_position_embeddings = getattr(ernie_config, 'max_position_embeddings', 4096)
            ernie_model = ErnieForCausalLM(ernie_config)

        # 加载我们提取的权重
        ernie_model.model.load_state_dict(llm_state)
        ernie_model.lm_head.load_state_dict(lm_head_state)

        # 保存为 HuggingFace 格式
        ernie_model.save_pretrained(hf_dir)

        # 复制 tokenizer 文件
        tokenizer = AutoTokenizer.from_pretrained(input_path, trust_remote_code=True)
        tokenizer.save_pretrained(hf_dir)

        print(f"✅ HuggingFace 检查点创建完成: {hf_dir}")

    # 统计参数
    total_params = sum(param.numel() for param in llm_model.parameters())
    print(f"\n📊 语言模型统计:")
    print(f"总参数: {total_params:,}")
    print(f"   - 隐藏层数: {text_config.num_hidden_layers}")
    print(f"   - 注意力头数: {text_config.num_attention_heads}")
    print(f"   - 词汇表大小: {text_config.vocab_size}")

    # 清理内存
    del full_model, llm_model, lm_head
    torch.cuda.empty_cache() if torch.cuda.is_available() else None

    print("✅ 语言模型导出完成！")
    print(f"📁 输出目录: {output_path}")
    print("📋 包含文件:")
    for file in sorted(output_dir.glob("*")):
        if file.is_file():
            size_mb = file.stat().st_size / (1024 * 1024)
            print(f"   - {file.name}: {size_mb:.1f} MB")
    if create_hf_checkpoint:
        print("📋 HF 检查点文件:")
        for file in sorted(hf_dir.glob("*")):
            if file.is_file():
                size_mb = file.stat().st_size / (1024 * 1024)
                print(f"   - {file.name}: {size_mb:.1f} MB")
if __name__ == "__main__":
    parser = argparse.ArgumentParser(description="导出 PaddleOCR-VL 语言模型部分")
    parser.add_argument("--input-path", type=str, required=True,
                       help="输入的完整模型路径")
    parser.add_argument("--output-path", type=str, required=True,
                       help="输出路径")
    parser.add_argument("--no-hf-checkpoint", action="store_true",
                       help="不创建 HuggingFace 格式检查点")

    args = parser.parse_args()

    export_language_model(
        args.input_path,
        args.output_path,
        create_hf_checkpoint=not args.no_hf_checkpoint
    )