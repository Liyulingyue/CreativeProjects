#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
PaddleOCR-VL API 客户端 (独立版)
"""

import base64
import json
import requests
from typing import Optional

class PaddleOCRVLClient:
    def __init__(self, base_url: str = "http://localhost:7778"):
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
        """
        try:
            with open(image_path, "rb") as image_file:
                encoded_string = base64.b64encode(image_file.read()).decode('utf-8')
                # 简单推断 mime 类型
                ext = image_path.split('.')[-1].lower()
                mime = f"image/{ext}" if ext in ['png', 'jpg', 'jpeg', 'webp'] else "image/jpeg"
                return f"data:{mime};base64,{encoded_string}"
        except Exception as e:
            raise ValueError(f"无法读取图像文件 {image_path}: {e}")

    def chat_completion(
        self,
        text: str,
        image_path: Optional[str] = None,
        max_tokens: int = 1024,
        temperature: float = 0.7,
        stream: bool = False
    ) -> dict:
        """
        发送聊天完成请求
        """
        content = []
        if text:
            content.append({"type": "text", "text": text})

        if image_path:
            image_url = self.encode_image_to_base64(image_path)
            content.append({
                "type": "image_url",
                "image_url": {"url": image_url}
            })

        payload = {
            "messages": [{"role": "user", "content": content}],
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
        处理流式响应并打印到终端
        """
        full_content = ""
        print("流式响应开始:")
        try:
            for line in response.iter_lines():
                if line:
                    line = line.decode('utf-8')
                    if line.startswith('data: '):
                        data = line[6:]
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
        return {"content": full_content, "streamed": True}

    def list_models(self) -> dict:
        """
        获取可用模型列表
        """
        try:
            response = self.session.get(f"{self.base_url}/v1/models")
            response.raise_for_status()
            return response.json()
        except requests.exceptions.RequestException as e:
            raise Exception(f"获取模型列表失败: {e}")
