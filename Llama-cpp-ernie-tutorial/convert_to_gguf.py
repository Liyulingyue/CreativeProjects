#!/usr/bin/env python3
"""
ERNIE 4.5 到 GGUF 转换脚本
调用 llama.cpp 的 convert.py 将 ERNIE 4.5 模型转换为 GGUF 格式
"""

import argparse
import os
import subprocess
import sys
from pathlib import Path

def convert_to_gguf(input_path: str, output_path: str, llama_cpp_path: str = "./llama.cpp",
                   outtype: str = "f16"):
    """
    使用 llama.cpp 的 convert.py 将 ERNIE 4.5 模型转换为 GGUF 格式

    Args:
        input_path: 输入模型路径
        output_path: 输出 GGUF 文件路径
        llama_cpp_path: llama.cpp 仓库路径
        outtype: 输出类型 (f16, q8_0, q4_0, etc.)
    """

    print(f"开始转换 ERNIE 模型: {input_path}")

    # 检查输入路径
    if not Path(input_path).exists():
        print(f"错误: 输入路径不存在: {input_path}")
        return False

    # 检查 llama.cpp 路径
    convert_script = Path(llama_cpp_path) / "convert.py"
    if not convert_script.exists():
        print(f"错误: llama.cpp convert.py 未找到: {convert_script}")
        print("请确保已克隆 llama.cpp 仓库并位于正确路径")
        return False

    # 确保输出目录存在
    output_dir = Path(output_path).parent
    output_dir.mkdir(parents=True, exist_ok=True)

    print(f"正在转换为 GGUF 格式 ({outtype})...")

    # 构建转换命令
    cmd = [
        sys.executable, str(convert_script),
        input_path,
        "--outfile", output_path,
        "--outtype", outtype
    ]

    print(f"执行命令: {' '.join(cmd)}")

    try:
        result = subprocess.run(cmd, capture_output=True, text=True, cwd=llama_cpp_path)

        if result.returncode == 0:
            print("转换成功完成！")
            print(f"GGUF 文件: {output_path}")
            return True
        else:
            print("转换失败")
            print("STDOUT:", result.stdout)
            print("STDERR:", result.stderr)
            return False

    except Exception as e:
        print(f"转换过程中出错: {e}")
        return False

def main():
    parser = argparse.ArgumentParser(description="ERNIE 4.5 到 GGUF 转换工具")
    parser.add_argument("--input", required=True, help="输入 ERNIE 模型路径")
    parser.add_argument("--output", required=True, help="输出 GGUF 文件路径")
    parser.add_argument("--llama-cpp", default="./llama.cpp", help="llama.cpp 仓库路径")
    parser.add_argument("--outtype", default="f16", help="输出类型 (f16, q8_0, q4_0, etc.)")

    args = parser.parse_args()

    success = convert_to_gguf(args.input, args.output, args.llama_cpp, args.outtype)

    if not success:
        sys.exit(1)

if __name__ == "__main__":
    main()