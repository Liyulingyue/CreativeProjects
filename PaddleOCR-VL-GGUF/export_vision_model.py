#!/usr/bin/env python3
"""
导出 PaddleOCR-VL 视觉模型的微缩版
只包含视觉编码器和投影层，去除 LLM 部分以减少内存占用
"""

import torch
import torch.nn as nn
from transformers import AutoProcessor, AutoModelForCausalLM
import argparse
import os
from pathlib import Path
import json

class PaddleOCRVisionOnlyModel(nn.Module):
    """
    微缩版 PaddleOCR-VL 模型，只包含视觉编码器和投影层
    """
    def __init__(self, visual_encoder, projector, config):
        super().__init__()
        self.visual = visual_encoder
        self.mlp_AR = projector
        self.config = config

    def forward(self, pixel_values, image_grid_thw, position_ids, vision_return_embed_list=True,
                interpolate_pos_encoding=True, sample_indices=None, cu_seqlens=None,
                return_pooler_output=False, use_rope=True, window_size=-1):
        """
        前向传播，只处理视觉输入
        """
        return self.visual(
            pixel_values=pixel_values,
            image_grid_thw=image_grid_thw,
            position_ids=position_ids,
            vision_return_embed_list=vision_return_embed_list,
            interpolate_pos_encoding=interpolate_pos_encoding,
            sample_indices=sample_indices,
            cu_seqlens=cu_seqlens,
            return_pooler_output=return_pooler_output,
            use_rope=use_rope,
            window_size=window_size,
        )

def export_vision_model(input_path: str, output_path: str):
    """
    导出视觉模型微缩版

    Args:
        input_path: 原始完整模型路径
        output_path: 输出路径
    """
    print(f"正在加载完整模型: {input_path}")
    full_model = AutoModelForCausalLM.from_pretrained(
        input_path,
        trust_remote_code=True,
        torch_dtype=torch.float32,
        low_cpu_mem_usage=True
    ).to("cpu")

    print("提取视觉组件...")
    visual_encoder = full_model.visual
    projector = full_model.mlp_AR
    config = full_model.config

    # 创建微缩版模型
    vision_model = PaddleOCRVisionOnlyModel(visual_encoder, projector, config)

    # 设置为评估模式
    vision_model.eval()

    # 保存模型
    os.makedirs(output_path, exist_ok=True)

    print(f"保存微缩版模型到: {output_path}")
    # 分别保存视觉编码器和投影层的状态
    vision_state = visual_encoder.state_dict()
    projector_state = projector.state_dict()

    torch.save({
        'visual_encoder': vision_state,
        'projector': projector_state,
        'config': config.to_dict()
    }, os.path.join(output_path, 'vision_model.pt'))

    # 复制 processor 和 tokenizer 文件
    processor = AutoProcessor.from_pretrained(input_path, trust_remote_code=True)
    processor.save_pretrained(output_path)

    # 抽取并保存必要的代码文件，实现模型独立加载
    import shutil
    print("抽取模型定义代码...")
    code_files = ['configuration_paddleocr_vl.py', 'modeling_paddleocr_vl.py']
    for filename in code_files:
        src_file = os.path.join(input_path, filename)
        if os.path.exists(src_file):
            shutil.copy(src_file, os.path.join(output_path, filename))
            print(f"   - 已抽取: {filename}")
        else:
            print(f"   - 警告: 未找到 {filename}")

    # 保存配置信息
    with open(os.path.join(output_path, 'model_info.json'), 'w') as f:
        json.dump({
            'model_type': 'paddleocr_vl_vision_only',
            'original_model': input_path,
            'components': ['visual_encoder', 'projector', 'processor'],
            'torch_dtype': 'float32'
        }, f, indent=2)

    # 清理内存
    del full_model
    torch.cuda.empty_cache() if torch.cuda.is_available() else None

    print("✅ 视觉模型导出完成！")
    print(f"📁 输出目录: {output_path}")
    print("📋 包含文件:")
    for file in os.listdir(output_path):
        print(f"   - {file}")

def load_vision_model(model_path: str, device: str = "cpu"):
    """
    加载微缩版视觉模型

    Args:
        model_path: 模型路径
        device: 设备 ('cpu' 或 'cuda')

    Returns:
        vision_model: 视觉模型
        processor: processor
    """
    print("正在直接加载微缩版视觉模型...")

    # 加载保存的状态
    checkpoint = torch.load(os.path.join(model_path, 'vision_model.pt'), map_location=device, weights_only=False)

    # 加载配置
    config_dict = checkpoint['config']
    import sys
    sys.path.append('PaddlePaddle/PaddleOCR-VL')
    from configuration_paddleocr_vl import PaddleOCRVLConfig
    config = PaddleOCRVLConfig(**config_dict)

    # 获取原始模型路径来重建结构
    with open(os.path.join(model_path, 'model_info.json'), 'r') as f:
        info = json.load(f)
    original_path = info['original_model']

    # 快速加载完整模型获取结构
    full_model = AutoModelForCausalLM.from_pretrained(
        original_path,
        trust_remote_code=True,
        torch_dtype=torch.float32,
        low_cpu_mem_usage=True
    ).to(device)

    # 加载保存的状态到组件中
    full_model.visual.load_state_dict(checkpoint['visual_encoder'])
    full_model.mlp_AR.load_state_dict(checkpoint['projector'])

    # 创建微缩版模型
    vision_model = PaddleOCRVisionOnlyModel(
        full_model.visual,
        full_model.mlp_AR,
        config
    )

    vision_model.to(device)
    vision_model.eval()

    # 清理临时模型（保留视觉组件）
    del full_model.model
    del full_model.lm_head
    del full_model
    torch.cuda.empty_cache() if torch.cuda.is_available() else None

    # 加载 processor
    processor = AutoProcessor.from_pretrained(model_path, trust_remote_code=True)

    return vision_model, processor

if __name__ == "__main__":
    parser = argparse.ArgumentParser(description="导出 PaddleOCR-VL 视觉模型微缩版")
    parser.add_argument("--input-path", type=str, required=True,
                       help="原始完整模型路径")
    parser.add_argument("--output-path", type=str, required=True,
                       help="输出路径")

    args = parser.parse_args()

    export_vision_model(args.input_path, args.output_path)