#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
提取 PaddleOCR-VL 的语言模型权重,并准备后续 GGUF/llama.cpp 转换所需的资产。

脚本会生成以下内容:

1. 原始的权重切分 (`llm_model.pt`, `lm_head.pt`, `llm_config.json`)
2. 可选的 Hugging Face 风格检查点 (`hf_model/`)

这样即可直接调用 llama.cpp 的 `convert_hf_to_gguf.py` 脚本。
"""

import argparse
import copy
import importlib
import json
from pathlib import Path
from typing import Optional

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
    
    base_config = AutoConfig.from_pretrained(model_path, trust_remote_code=True)
    text_config = sanitize_text_config(base_config)
    
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
    llm_state = llm_model.state_dict()
    torch.save(llm_state, llm_weights_path)

    print(f"保存 LM Head 到: {lm_head_path}")
    lm_head_state = lm_head.state_dict()
    torch.save(lm_head_state, lm_head_path)
    
    # 保存配置
    print(f"保存配置到: {config_path}")
    with open(config_path, 'w', encoding='utf-8') as f:
        json.dump(text_config.to_dict(), f, indent=2, ensure_ascii=False)
    
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
    
    return output_dir, llm_state, lm_head_state, text_config, full_model.__class__.__module__


def export_to_hf_checkpoint(
    model_path,
    config,
    llm_state,
    lm_head_state,
    hf_output_dir,
    module_name: Optional[str],
):
    """将提取的语言模型重新封装成 Hugging Face 兼容的检查点。"""

    hf_output_dir = Path(hf_output_dir)
    hf_output_dir.mkdir(parents=True, exist_ok=True)

    print(f"\n导出 Hugging Face 检查点到: {hf_output_dir}")

    ernie_cls = None
    if module_name:
        try:
            module = importlib.import_module(module_name)
            ernie_cls = getattr(module, "Ernie4_5ForCausalLM", None)
        except ModuleNotFoundError as exc:  # pragma: no cover - env dependent
            print(f"警告: 无法导入模块 {module_name}: {exc}")

    try:
        # 直接基于配置实例化模型,避免重复加载整套多模态权重。
        if ernie_cls is not None:
            hf_model = ernie_cls(copy.deepcopy(config))
        else:
            hf_model = AutoModelForCausalLM.from_config(
                config,
                trust_remote_code=True,
            )
        hf_model.model.load_state_dict(llm_state, strict=True)
        hf_model.lm_head.load_state_dict(lm_head_state, strict=True)
    except Exception as exc:  # noqa: BLE001 - 需要捕获并回退
        print("警告: 基于配置构建模型失败, 将回退到 from_pretrained 路径。")
        print(f"详细信息: {exc}")
        hf_model = AutoModelForCausalLM.from_pretrained(
            model_path,
            trust_remote_code=True,
            torch_dtype=torch.float32,
            low_cpu_mem_usage=True,
        )
        hf_model.model.load_state_dict(llm_state, strict=True)
        hf_model.lm_head.load_state_dict(lm_head_state, strict=True)

    hf_model.config.architectures = ["Ernie4_5ForCausalLM"]
    hf_model.config.model_type = "ernie4_5"
    hf_model.config.auto_map = {
        "AutoConfig": "configuration_paddleocr_vl.PaddleOCRVLConfig",
        "AutoModelForCausalLM": "modeling_paddleocr_vl.Ernie4_5ForCausalLM",
    }
    hf_model.save_pretrained(hf_output_dir, safe_serialization=True)

    try:
        tokenizer = AutoTokenizer.from_pretrained(
            model_path,
            trust_remote_code=True,
            use_fast=True,
        )
    except (ValueError, ImportError) as exc:  # noqa: PERF203 - narrow fallback
        print("警告: Fast Tokenizer 初始化失败, 回退至 slow 版本。")
        print(f"详细信息: {exc}")
        try:
            tokenizer = AutoTokenizer.from_pretrained(
                model_path,
                trust_remote_code=True,
                use_fast=False,
            )
        except ImportError as slow_exc:  # pragma: no cover - dependency guidance
            raise RuntimeError(
                "无法加载 slow tokenizer, 请先安装 sentencepiece 和 protobuf 后重试。"
            ) from slow_exc
    tokenizer.save_pretrained(hf_output_dir)

    print("Hugging Face 检查点导出完成。")


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
    parser.add_argument(
        "--hf-output-dir",
        type=str,
        default=None,
        help="额外导出 Hugging Face 检查点的目录 (默认: <output-path>/hf_model)"
    )
    parser.add_argument(
        "--no-hf-export",
        action="store_true",
        help="仅保存原始切分权重, 不生成 Hugging Face 检查点"
    )
    
    args = parser.parse_args()
    
    print("=" * 60)
    print("PaddleOCR-VL LLM 提取工具")
    print("=" * 60)
    
    # 提取权重
    output_dir, llm_state, lm_head_state, config, module_name = extract_llm_weights(
        args.model_path,
        args.output_path,
    )

    if not args.no_hf_export:
        hf_output_dir = Path(args.hf_output_dir) if args.hf_output_dir else output_dir / "hf_model"
        export_to_hf_checkpoint(
            args.model_path,
            config,
            llm_state,
            lm_head_state,
            hf_output_dir,
            module_name,
        )
    
    print("\n" + "=" * 60)
    print("提取完成!")
    print("=" * 60)
    print("后续转换与量化步骤请参考项目 README.md 中的 llama.cpp 部分。")


if __name__ == "__main__":
    main()
