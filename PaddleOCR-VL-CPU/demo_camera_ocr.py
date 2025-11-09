#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
使用摄像头捕获图像并使用本地API进行OCR
"""

import cv2
import base64
import sys
from openai import OpenAI
from pathlib import Path
import time

def capture_image(filename='captured_image.jpg', camera_index=0):
    """
    从摄像头抓取一张图像并保存到文件

    Args:
        filename: 保存的文件名 (默认: captured_image.jpg)
        camera_index: 摄像头索引 (默认: 0)
    """
    # 打开摄像头
    cap = cv2.VideoCapture(camera_index)
    
    if not cap.isOpened():
        print(f"无法打开摄像头 {camera_index}")
        return False
    
    print("摄像头已打开，正在捕获图像...")
    
    ret, frame = cap.read()
    if not ret:
        print("无法读取帧")
        cap.release()
        return False
    
    cv2.imwrite(filename, frame)
    print(f"图像已保存到 {filename}")
    
    # 释放资源
    cap.release()
    return True

def encode_image_to_base64(image_path: str) -> str:
    """
    将图像文件编码为base64字符串

    Args:
        image_path: 图像文件路径

    Returns:
        base64编码的图像数据
    """
    try:
        with open(image_path, "rb") as image_file:
            encoded_string = base64.b64encode(image_file.read()).decode('utf-8')
            return f"data:image/jpeg;base64,{encoded_string}"
    except Exception as e:
        raise ValueError(f"无法读取图像文件 {image_path}: {e}")

def ocr_with_openai(image_path: str, base_url: str = "http://localhost:7777/v1", prompt: str = "Extract all the text from this image. If there is no text, say 'No text found'.") -> str:
    """
    使用OpenAI兼容API进行OCR

    Args:
        image_path: 图像文件路径
        base_url: API服务器基础URL
        prompt: 提示文本

    Returns:
        提取的文本
    """
    client = OpenAI(
        api_key="",  # 空字符串
        base_url=base_url
    )

    # 编码图像
    encode_start = time.time()
    base64_image = encode_image_to_base64(image_path)
    encode_end = time.time()
    print(f"图像编码耗时: {encode_end - encode_start:.2f} 秒")

    try:
        response = client.chat.completions.create(
            model="PaddleOCR-VL",
            messages=[
                {
                    "role": "user",
                    "content": [
                        {"type": "text", "text": prompt},
                        {
                            "type": "image_url",
                            "image_url": {"url": base64_image}
                        }
                    ]
                }
            ],
            max_tokens=1000,
            temperature=0.1
        )

        return response.choices[0].message.content.strip()

    except Exception as e:
        raise Exception(f"API调用失败: {e}")

def main():
    # 解析命令行参数
    import argparse
    parser = argparse.ArgumentParser(description="摄像头OCR演示")
    parser.add_argument("--camera", type=int, default=0, help="摄像头索引 (默认: 0)")
    parser.add_argument("--output", default="captured_image.jpg", help="输出图像文件名 (默认: captured_image.jpg)")
    parser.add_argument("--prompt", default="Extract all the text from this image. If there is no text, say 'No text found'.", help="OCR提示文本")
    parser.add_argument("--url", default="http://localhost:7777/v1", help="API服务器URL (默认: http://localhost:7777/v1)")
    
    args = parser.parse_args()
    
    start_time = time.time()
    
    try:
        # 捕获图像
        capture_start = time.time()
        if not capture_image(args.output, args.camera):
            print("图像捕获失败")
            sys.exit(1)
        capture_end = time.time()
        print(f"图像捕获耗时: {capture_end - capture_start:.2f} 秒")
        
        # 进行OCR
        ocr_start = time.time()
        print(f"正在调用API {args.url} 进行OCR...")
        ocr_result = ocr_with_openai(args.output, args.url, args.prompt)
        ocr_end = time.time()
        print(f"OCR调用耗时: {ocr_end - ocr_start:.2f} 秒")
        
        print("\n=== OCR结果 ===")
        print(ocr_result)
        print("===============")
        
        end_time = time.time()
        print(f"总耗时: {end_time - start_time:.2f} 秒")
        
        # 可选：删除临时图像文件
        # os.remove(args.output)
        
    except Exception as e:
        print(f"错误: {e}")
        sys.exit(1)

if __name__ == "__main__":
    main()