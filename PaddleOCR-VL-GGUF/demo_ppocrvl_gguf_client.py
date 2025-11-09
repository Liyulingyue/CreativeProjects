#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
PaddleOCR-VL GGUF 版本测试客户端
"""

import sys
import os
import time

# 复用 CPU 版本客户端实现
cpu_client_dir = os.path.join(os.path.dirname(__file__), "..", "PaddleOCR-VL-CPU")
sys.path.insert(0, os.path.abspath(cpu_client_dir))
from demo_ppocrvl_client import PaddleOCRVLClient
import argparse


def main():
    parser = argparse.ArgumentParser(description="PaddleOCR-VL GGUF API 客户端测试")
    parser.add_argument("--url", default="http://localhost:7778", help="API服务器URL (GGUF版本使用7778端口)")
    parser.add_argument("--text", default="OCR:", help="文本提示")
    parser.add_argument("--image", help="图像文件路径")
    parser.add_argument("--max-tokens", type=int, default=1024, help="最大生成token数")
    parser.add_argument("--temperature", type=float, default=0.7, help="温度参数")
    parser.add_argument("--stream", action="store_true", help="启用流式响应")
    parser.add_argument("--list-models", action="store_true", help="列出可用模型")

    args = parser.parse_args()

    client = PaddleOCRVLClient(args.url)

    try:
        if args.list_models:
            models = client.list_models()
            print("可用模型:")
            for model in models.get('data', []):
                print(f"- {model['id']}")
                if 'capabilities' in model:
                    caps = model['capabilities']
                    print(f"  后端: {caps.get('backend', 'unknown')}")
                    print(f"  视觉: {caps.get('vision', False)}")
        else:
            if not args.image:
                print("警告: 未提供图像，将进行纯文本测试")
            
            print(f"\n正在测试 GGUF 后端...")
            print(f"服务器: {args.url}")
            print(f"提示: {args.text}")
            if args.image:
                print(f"图像: {args.image}")
            print(f"流式: {args.stream}")
            print("-" * 60)
            
            start_time = time.time()
            response = client.chat_completion(
                text=args.text,
                image_path=args.image,
                max_tokens=args.max_tokens,
                temperature=args.temperature,
                stream=args.stream
            )
            end_time = time.time()
            print(f"\n消耗时间: {end_time - start_time:.2f} 秒")

            if args.stream:
                # 流式响应已经在_handle_stream_response中打印
                pass
            else:
                content = response.get('choices', [{}])[0].get('message', {}).get('content', '')
                usage = response.get('usage', {})

                print("\n响应内容:")
                print(content)
                print("\n使用统计:")
                print(f"- 提示tokens: {usage.get('prompt_tokens', 'N/A')}")
                print(f"- 完成tokens: {usage.get('completion_tokens', 'N/A')}")
                print(f"- 总tokens: {usage.get('total_tokens', 'N/A')}")

    except Exception as e:
        print(f"错误: {e}")
        import traceback
        traceback.print_exc()
        sys.exit(1)


if __name__ == "__main__":
    main()

# python demo_ppocrvl_gguf_client.py --image test.png