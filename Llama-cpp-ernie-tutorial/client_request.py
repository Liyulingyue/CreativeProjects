#!/usr/bin/env python3
"""
ERNIE GGUF API 客户端测试工具
"""

import argparse
import json
import requests
import sys

def chat_completion(url: str, prompt: str, model: str = "ernie-gguf",
                   max_tokens: int = 1024, temperature: float = 0.7, stream: bool = False):
    """发送聊天完成请求"""

    payload = {
        "model": model,
        "messages": [
            {"role": "user", "content": prompt}
        ],
        "max_tokens": max_tokens,
        "temperature": temperature,
        "stream": stream
    }

    headers = {"Content-Type": "application/json"}

    try:
        if stream:
            # 流式响应
            response = requests.post(f"{url}/v1/chat/completions",
                                   json=payload, headers=headers, stream=True)

            if response.status_code != 200:
                print(f"错误: {response.status_code}")
                print(response.text)
                return

            print("响应:")
            for line in response.iter_lines():
                if line:
                    line = line.decode('utf-8')
                    if line.startswith('data: '):
                        data = line[6:]
                        if data == '[DONE]':
                            break
                        try:
                            chunk = json.loads(data)
                            if chunk["choices"]:
                                content = chunk["choices"][0]["delta"].get("content", "")
                                print(content, end="", flush=True)
                        except json.JSONDecodeError:
                            continue
            print("\n")
        else:
            # 非流式响应
            response = requests.post(f"{url}/v1/chat/completions",
                                   json=payload, headers=headers)

            if response.status_code == 200:
                result = response.json()
                content = result["choices"][0]["message"]["content"]
                print("响应:")
                print(content)
                print(f"\n使用 tokens: {result['usage']['total_tokens']}")
            else:
                print(f"错误: {response.status_code}")
                print(response.text)

    except requests.exceptions.RequestException as e:
        print(f"请求失败: {e}")

def list_models(url: str):
    """列出可用模型"""
    try:
        response = requests.get(f"{url}/v1/models")
        if response.status_code == 200:
            models = response.json()
            print("可用模型:")
            for model in models["data"]:
                print(f"- {model['id']} (by {model['owned_by']})")
        else:
            print(f"错误: {response.status_code}")
            print(response.text)
    except requests.exceptions.RequestException as e:
        print(f"请求失败: {e}")

def main():
    parser = argparse.ArgumentParser(description="ERNIE GGUF API 客户端")
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
            url=args.url,
            prompt=args.prompt,
            model=args.model,
            max_tokens=args.max_tokens,
            temperature=args.temperature,
            stream=args.stream
        )

if __name__ == "__main__":
    main()