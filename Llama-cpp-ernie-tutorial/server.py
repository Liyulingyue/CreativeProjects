#!/usr/bin/env python3
"""
ERNIE GGUF 服务器启动脚本
启动 llama.cpp 的 server 二进制以提供 OpenAI 兼容的 API
"""

import argparse
import os
import subprocess
import sys
from pathlib import Path

def start_server(model_path: str, llama_cpp_path: str = "./llama.cpp",
                 host: str = "0.0.0.0", port: int = 8000, ctx_size: int = 4096,
                 n_threads: int = 4):
    """
    启动 llama.cpp server

    Args:
        model_path: GGUF 模型文件路径
        llama_cpp_path: llama.cpp 仓库路径
        host: 服务器主机
        port: 服务器端口
        ctx_size: 上下文大小
        n_threads: CPU 线程数
    """

    # 检查模型文件
    if not Path(model_path).exists():
        print(f"错误: 模型文件不存在: {model_path}")
        return False

    # 检查 server 二进制
    server_binary = Path(llama_cpp_path) / "build" / "bin" / "llama-server.exe"
    if not server_binary.exists():
        print(f"错误: llama.cpp server 二进制未找到: {server_binary}")
        print("请确保已编译 llama.cpp")
        return False

    print(f"启动 llama.cpp server...")
    print(f"模型: {model_path}")
    print(f"主机: {host}:{port}")
    print(f"上下文大小: {ctx_size}")
    print(f"线程数: {n_threads}")

    # 构建命令
    cmd = [
        str(server_binary),
        "--model", model_path,
        "--host", host,
        "--port", str(port),
        "--ctx-size", str(ctx_size),
        "--threads", str(n_threads)
    ]

    print(f"执行命令: {' '.join(cmd)}")

    try:
        # 启动服务器（阻塞）
        subprocess.run(cmd, cwd=llama_cpp_path)
        return True
    except KeyboardInterrupt:
        print("\n服务器已停止")
        return True
    except Exception as e:
        print(f"启动服务器失败: {e}")
        return False

def main():
    parser = argparse.ArgumentParser(description="ERNIE GGUF 服务器启动器")
    parser.add_argument("--model", default="./ernie_q4_k_m.gguf", help="GGUF 模型文件路径")
    parser.add_argument("--llama-cpp", default="./llama.cpp", help="llama.cpp 仓库路径")
    parser.add_argument("--host", default="0.0.0.0", help="服务器主机")
    parser.add_argument("--port", type=int, default=8000, help="服务器端口")
    parser.add_argument("--ctx-size", type=int, default=4096, help="上下文大小")
    parser.add_argument("--threads", type=int, default=4, help="CPU 线程数")

    args = parser.parse_args()

    success = start_server(
        model_path=args.model,
        llama_cpp_path=args.llama_cpp,
        host=args.host,
        port=args.port,
        ctx_size=args.ctx_size,
        n_threads=args.threads
    )

    if not success:
        sys.exit(1)

if __name__ == "__main__":
    main()