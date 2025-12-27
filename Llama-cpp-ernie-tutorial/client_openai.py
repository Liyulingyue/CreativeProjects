#!/usr/bin/env python3
"""
ERNIE GGUF OpenAI API 客户端测试工具
使用 OpenAI 库调用本地服务器
"""

import argparse
import sys
from openai import OpenAI

def chat_completion(base_url: str, prompt: str, model: str = "ernie-gguf",
                   max_tokens: int = 1024, temperature: float = 0.7, stream: bool = False):
    """发送聊天完成请求"""

    client = OpenAI(
        base_url=f"{base_url}/v1",
        api_key="not-needed"  # 本地服务器不需要 API key
    )

    try:
        if stream:
            # 流式响应
            response = client.chat.completions.create(
                model=model,
                messages=[{"role": "user", "content": prompt}],
                max_tokens=max_tokens,
                temperature=temperature,
                stream=True
            )

            print("响应:")
            for chunk in response:
                if chunk.choices:
                    content = chunk.choices[0].delta.content
                    if content:
                        print(content, end="", flush=True)
            print("\n")
        else:
            # 非流式响应
            response = client.chat.completions.create(
                model=model,
                messages=[{"role": "user", "content": prompt}],
                max_tokens=max_tokens,
                temperature=temperature,
                stream=False
            )

            content = response.choices[0].message.content
            print("响应:")
            print(content)
            print(f"\n使用 tokens: {response.usage.total_tokens}")

    except Exception as e:
        print(f"请求失败: {e}")

def list_models(base_url: str):
    """列出可用模型"""
    try:
        client = OpenAI(
            base_url=f"{base_url}/v1",
            api_key="not-needed"
        )

        models = client.models.list()
        print("可用模型:")
        for model in models.data:
            print(f"- {model.id} (by {model.owned_by})")
    except Exception as e:
        print(f"请求失败: {e}")

def main():
    parser = argparse.ArgumentParser(description="ERNIE GGUF OpenAI API 客户端")
    parser.add_argument("--url", default="http://localhost:8000", help="API 服务器 URL")
    parser.add_argument("--prompt", default="你好，请介绍一下自己。", help="要发送的提示文本")
    parser.add_argument("--model", default="ernie-gguf", help="模型名称")
    parser.add_argument("--max-tokens", type=int, default=1024, help="最大生成 token 数")
    parser.add_argument("--temperature", type=float, default=0.7, help="温度参数")
    parser.add_argument("--stream", action="store_true", help="启用流式响应")
    parser.add_argument("--list-models", action="store_true", help="列出可用模型")

    args = parser.parse_args()

    if args.list_models:
        list_models(args.url)
    else:
        chat_completion(
            base_url=args.url,
            prompt=args.prompt,
            model=args.model,
            max_tokens=args.max_tokens,
            temperature=args.temperature,
            stream=args.stream
        )

if __name__ == "__main__":
    main()