import os
import sys
import ctypes
import json
import base64
import argparse
from openai import OpenAI

# 1. 设置壁纸的核心逻辑 (Windows API)
def set_wallpaper(image_path):
    abs_path = os.path.abspath(image_path)
    if not os.path.exists(abs_path):
        print(f"错误: 路径 {abs_path} 不存在。")
        return False
    
    # SPI_SETDESKWALLPAPER = 20
    # SPIF_UPDATEINIFILE = 0x01 | SPIF_SENDWININICHANGE = 0x02
    ctypes.windll.user32.SystemParametersInfoW(20, 0, abs_path, 0x01 | 0x02)
    print(f"壁纸已成功设置为: {abs_path}")
    return True

# 2. 调用 API 生成图片
def generate_wallpaper(args):
    client = OpenAI(
        api_key=args.api_key,
        base_url=args.base_url
    )
    
    print(f"正在生成图片，提示词: '{args.prompt}'...")
    try:
        response = client.images.generate(
            model=args.model,
            prompt=args.prompt,
            n=1,
            size=args.size,
            response_format="b64_json",
            extra_body={
                "seed": args.seed, 
                "use_pe": True, 
                "num_inference_steps": 8, 
                "guidance_scale": 1.0
            }
        )
        
        # 处理 Base64 数据
        b64_data = response.data[0].b64_json
        image_bytes = base64.b64decode(b64_data)
        
        if not os.path.exists(args.save_dir):
            os.makedirs(args.save_dir)
            
        # 使用固定文件名 wallpaper.png 避免存储空间无限扩大
        file_name = "wallpaper.png"
        image_path = os.path.join(args.save_dir, file_name)
        
        with open(image_path, 'wb') as handler:
            handler.write(image_bytes)
            
        return image_path
    except Exception as e:
        print(f"生成图片失败: {e}")
        return None

# 3. 主流程
def main():
    parser = argparse.ArgumentParser(description="AIWallpaper Skill - 百度文心 API 生成并设置桌面壁纸")
    
    # 必须参数或通过环境变量获取
    parser.add_argument("--api_key", type=str, default=os.getenv("BAIDU_API_KEY"), help="API Key (也可通过环境变量 BAIDU_API_KEY 设置)")
    parser.add_argument("--prompt", type=str, help="提示词 (如果没有提供，将进入交互模式)")
    
    # 可选参数
    parser.add_argument("--base_url", type=str, default="https://aistudio.baidu.com/llm/lmapi/v3", help="API Base URL")
    parser.add_argument("--model", type=str, default="ernie-image-turbo", help="生成模型")
    parser.add_argument("--save_dir", type=str, default="assets", help="保存目录")
    parser.add_argument("--size", type=str, default="1024x1024", help="图片尺寸")
    parser.add_argument("--seed", type=int, default=42, help="随机种子")

    args = parser.parse_args()

    # API Key 检查
    if not args.api_key:
        print("错误: 未提供 API Key。请通过 --api_key 参数或 BAIDU_API_KEY 环境变量提供。")
        return

    # 提示词检查
    if not args.prompt:
        args.prompt = input("请输入壁纸提示词 (例如 '赛博朋克风格的赛车'): ")
        if not args.prompt:
            print("错误: 提示词不能为空。")
            return
        
    image_path = generate_wallpaper(args)
    if image_path:
        set_wallpaper(image_path)

if __name__ == "__main__":
    main()
