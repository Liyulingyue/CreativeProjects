import os
import sys
import torch
import torch.nn as nn
from transformers import AutoProcessor
import json
import importlib.util

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

def _load_as_package_module(module_name, file_path, package_name):
    """
    辅助函数：将一个 py 文件加载为特定包下的模块，以支持相对导入
    """
    full_name = f"{package_name}.{module_name}" if package_name else module_name
    spec = importlib.util.spec_from_file_location(full_name, file_path)
    module = importlib.util.module_from_spec(spec)
    module.__package__ = package_name
    sys.modules[full_name] = module
    spec.loader.exec_module(module)
    return module

def load_vision_model(model_path: str, device: str = "cpu"):
    """
    加载微缩版视觉模型，完全不加载原始完整模型 (AutoModelForCausalLM)
    
    Args:
        model_path: 导出的微缩版模型目录路径 (包含 vision_model.pt)
        device: 加载设备
    """
    print(f"=== 独立加载微缩版视觉模型 ===")
    
    current_dir = os.path.dirname(os.path.abspath(__file__))
    
    # 仅从模型目录加载（要求执行了文件抽取，实现完全脱离 pp 仓库）
    pp_path = model_path
    
    if not os.path.exists(os.path.join(pp_path, "modeling_paddleocr_vl.py")):
        raise FileNotFoundError(f"找不到模型架构定义: {os.path.join(pp_path, 'modeling_paddleocr_vl.py')}。请确保已运行 export_vision_model.py 抽取代码。")

    # 使用 importlib 动态加载，避免相对导入错误
    try:
        # 创建一个虚拟包容器
        if "paddleocr_vl" not in sys.modules:
            pkg = importlib.util.module_from_spec(importlib.util.spec_from_loader("paddleocr_vl", None))
            pkg.__path__ = [pp_path]
            sys.modules["paddleocr_vl"] = pkg
        
        config_mod = _load_as_package_module(
            "configuration_paddleocr_vl", 
            os.path.join(pp_path, "configuration_paddleocr_vl.py"), 
            "paddleocr_vl"
        )
        modeling_mod = _load_as_package_module(
            "modeling_paddleocr_vl", 
            os.path.join(pp_path, "modeling_paddleocr_vl.py"), 
            "paddleocr_vl"
        )
        
        PaddleOCRVLConfig = config_mod.PaddleOCRVLConfig
        SiglipVisionModel = modeling_mod.SiglipVisionModel
        Projector = modeling_mod.Projector
        
    except Exception as e:
        print(f"加载模型组件失败: {e}")
        raise

    # 加载状态字典
    checkpoint_path = os.path.join(model_path, 'vision_model.pt')
    if not os.path.exists(checkpoint_path):
        raise FileNotFoundError(f"未找到视觉模型权重文件: {checkpoint_path}")
    
    print(f"正在加载权重: {checkpoint_path}")
    checkpoint = torch.load(checkpoint_path, map_location=device, weights_only=False)
    
    # 加载配置
    config_dict = checkpoint['config']
    config = PaddleOCRVLConfig(**config_dict)
    
    # 初始化视觉组件
    print("初始化视觉组件...")
    visual_encoder = SiglipVisionModel(config.vision_config)
    projector = Projector(config, config.vision_config)
    
    # 应用权重
    print("加载权重到组件...")
    visual_encoder.load_state_dict(checkpoint['visual_encoder'])
    projector.load_state_dict(checkpoint['projector'])
    
    # 创建模型包装
    model = PaddleOCRVisionOnlyModel(visual_encoder, projector, config)
    model.to(device)
    model.eval()
    
    # 加载处理器 (processor 仍然可以使用 AutoProcessor，但需要 trust_remote_code=True)
    # 因为 processor 指向的是导出的 model_path，那里有相应的处理逻辑
    print("加载处理器...")
    processor = AutoProcessor.from_pretrained(model_path, trust_remote_code=True)
    
    print("✅ 视觉模型独立加载成功")
    return model, processor

if __name__ == "__main__":
    # 测试脚本
    path = "vision_model"
    if os.path.exists(path):
        model, processor = load_vision_model(path)
        print("测试完成：成功加载模型")
    else:
        print(f"路径 {path} 不存在，跳过测试")

