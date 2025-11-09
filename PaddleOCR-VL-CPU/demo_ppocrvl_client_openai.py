#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
使用OpenAI兼容API读取图片并进行分析
"""

import base64
import sys
import time
from openai import OpenAI

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

def analyze_image_with_openai(image_path: str, base_url: str = "http://localhost:7777/v1", prompt: str = "描述这张图片的内容") -> str:
    """
    使用OpenAI兼容API分析图像

    Args:
        image_path: 图像文件路径
        base_url: API服务器基础URL
        prompt: 分析提示

    Returns:
        分析结果
    """
    client = OpenAI(
        api_key="",  # 空字符串
        base_url=base_url
    )

    # 编码图像
    base64_image = encode_image_to_base64(image_path)

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
            temperature=0.7
        )

        return response.choices[0].message.content.strip()

    except Exception as e:
        raise Exception(f"API调用失败: {e}")

def main():
    # 硬编码参数（简化版本）
    base_url = "http://localhost:7777/v1"
    image_path = "test.png"
    prompt = "OCR:"

    try:
        print(f"正在分析图片: {image_path}")
        print(f"API URL: {base_url}")
        print(f"提示: {prompt}")
        print("-" * 50)

        start_time = time.time()
        result = analyze_image_with_openai(image_path, base_url, prompt)
        end_time = time.time()
        processing_time = end_time - start_time

        print("分析结果:")
        print(result)
        print("-" * 50)
        print(f"处理时间: {processing_time:.2f} 秒")

    except Exception as e:
        print(f"错误: {e}")
        sys.exit(1)

if __name__ == "__main__":
    main()