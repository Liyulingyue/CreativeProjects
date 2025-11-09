#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
PaddleOCR-VL API 客户端
用于与 PaddleOCR-VL API 服务器交互
"""

import base64
import json
import requests
import argparse
from pathlib import Path
from typing import Optional, List
import sys

class PaddleOCRVLClient:
    def __init__(self, base_url: str = "http://localhost:7777"):
        """
        初始化客户端

        Args:
            base_url: API服务器的基础URL
        """
        self.base_url = base_url.rstrip('/')
        self.session = requests.Session()

    def encode_image_to_base64(self, image_path: str) -> str:
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

    def chat_completion(
        self,
        text: str,
        image_path: Optional[str] = None,
        max_tokens: int = 131072,
        temperature: float = 0.7,
        stream: bool = False
    ) -> dict:
        """
        发送聊天完成请求

        Args:
            text: 文本提示
            image_path: 图像文件路径（可选）
            max_tokens: 最大生成token数
            temperature: 温度参数
            stream: 是否启用流式响应

        Returns:
            API响应
        """
        # 构建消息内容
        content = []

        # 添加文本内容
        if text:
            content.append({
                "type": "text",
                "text": text
            })

        # 添加图像内容
        if image_path:
            image_url = self.encode_image_to_base64(image_path)
            content.append({
                "type": "image_url",
                "image_url": {
                    "url": image_url
                }
            })

        # 构建请求体
        payload = {
            "messages": [
                {
                    "role": "user",
                    "content": content
                }
            ],
            "max_tokens": max_tokens,
            "temperature": temperature,
            "stream": stream
        }

        try:
            response = self.session.post(
                f"{self.base_url}/v1/chat/completions",
                json=payload,
                stream=stream
            )
            response.raise_for_status()

            if stream:
                return self._handle_stream_response(response)
            else:
                return response.json()

        except requests.exceptions.RequestException as e:
            raise Exception(f"API请求失败: {e}")

    def _handle_stream_response(self, response) -> dict:
        """
        处理流式响应

        Args:
            response: 流式响应对象

        Returns:
            完整的响应内容
        """
        full_content = ""
        print("流式响应开始:")

        try:
            for line in response.iter_lines():
                if line:
                    line = line.decode('utf-8')
                    if line.startswith('data: '):
                        data = line[6:]  # 移除 'data: ' 前缀
                        if data == '[DONE]':
                            break

                        try:
                            chunk = json.loads(data)
                            delta = chunk.get('choices', [{}])[0].get('delta', {})
                            content = delta.get('content', '')

                            if content:
                                print(content, end='', flush=True)
                                full_content += content

                        except json.JSONDecodeError:
                            continue

        except Exception as e:
            print(f"\n流式响应处理出错: {e}")

        print("\n流式响应结束")

        # 返回模拟的完整响应
        return {
            "content": full_content,
            "streamed": True
        }

    def list_models(self) -> dict:
        """
        获取可用模型列表

        Returns:
            模型列表
        """
        try:
            response = self.session.get(f"{self.base_url}/v1/models")
            response.raise_for_status()
            return response.json()
        except requests.exceptions.RequestException as e:
            raise Exception(f"获取模型列表失败: {e}")


def main():
    parser = argparse.ArgumentParser(description="PaddleOCR-VL API 客户端")
    parser.add_argument("--url", default="http://localhost:7777", help="API服务器URL")
    parser.add_argument("--text", default="OCR:", help="文本提示")
    parser.add_argument("--image", help="图像文件路径")
    parser.add_argument("--max-tokens", type=int, default=131072, help="最大生成token数")
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
        else:
            response = client.chat_completion(
                text=args.text,
                image_path=args.image,
                max_tokens=args.max_tokens,
                temperature=args.temperature,
                stream=args.stream
            )

            if args.stream:
                # 流式响应已经在_handle_stream_response中打印
                pass
            else:
                content = response.get('choices', [{}])[0].get('message', {}).get('content', '')
                usage = response.get('usage', {})

                print("响应内容:")
                print(content)
                print("\n使用统计:")
                print(f"- 提示tokens: {usage.get('prompt_tokens', 'N/A')}")
                print(f"- 完成tokens: {usage.get('completion_tokens', 'N/A')}")
                print(f"- 总tokens: {usage.get('total_tokens', 'N/A')}")

    except Exception as e:
        print(f"错误: {e}")
        sys.exit(1)


if __name__ == "__main__":
    main()