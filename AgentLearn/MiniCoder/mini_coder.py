#!/usr/bin/env python
"""mini_coder.py - 智能代码助手 | AI-Powered Code Assistant"""
import os
import sys
from pathlib import Path
import json

# 尝试加载环境变量
try:
    from dotenv import load_dotenv
    load_dotenv()
except ImportError:
    pass

# 配置
API_KEY = os.getenv("MODEL_KEY", "")
BASE_URL = os.getenv("MODEL_URL", "https://api.openai.com/v1")
MODEL_NAME = os.getenv("MODEL_NAME", "gpt-4")

class MiniCoder:
    """最小化但功能完整的代码助手"""
    
    def __init__(self):
        self.system_prompt = """You are MiniCoder, an intelligent AI code assistant.

## 核心能力:
1. **代码生成**: 编写清晰、高效、符合最佳实践的代码
2. **代码解释**: 详细解释代码逻辑和实现原理
3. **Bug修复**: 分析错误并提供修复方案
4. **代码优化**: 改进代码性能、可读性和可维护性
5. **多语言支持**: Python, JavaScript, Java, C++, Go等

## 工作流程:
1. 理解用户需求和上下文
2. 分析问题并规划解决方案
3. 生成代码并添加详细注释
4. 提供使用示例和注意事项

## 编码规范:
- 遵循PEP 8 (Python) 或对应语言的最佳实践
- 添加类型提示和文档字符串
- 包含错误处理和边界情况
- 提供单元测试建议

## 响应格式:
- 先简要说明解决思路
- 然后提供完整代码
- 最后给出使用示例和注意事项"""
    
    def _call_llm(self, messages):
        """调用LLM API的内部方法"""
        if not API_KEY:
            return "⚠️  请先设置 MODEL_KEY 环境变量"
        
        try:
            from openai import OpenAI
            client = OpenAI(api_key=API_KEY, base_url=BASE_URL)
            
            response = client.chat.completions.create(
                model=MODEL_NAME,
                messages=messages,
                temperature=0.7,
            )
            return response.choices[0].message.content
        except ImportError:
            return "⚠️  请先安装 openai 库: pip install openai"
        except Exception as e:
            return f"⚠️  API调用失败: {str(e)}"
    
    def generate_code(self, prompt, language="python"):
        """生成代码"""
        messages = [
            {"role": "system", "content": self.system_prompt},
            {"role": "user", "content": f"请用{language}编写代码: {prompt}"}
        ]
        return self._call_llm(messages)
    
    def explain_code(self, code):
        """解释代码"""
        messages = [
            {"role": "system", "content": self.system_prompt},
            {"role": "user", "content": f"请详细解释这段代码:\n{code}"}
        ]
        return self._call_llm(messages)
    
    def fix_bug(self, error_message, code_context):
        """修复bug"""
        messages = [
            {"role": "system", "content": self.system_prompt},
            {"role": "user", "content": f"错误信息: {error_message}\n代码上下文: {code_context}\n请分析并提供修复方案"}
        ]
        return self._call_llm(messages)
    
    def optimize_code(self, code):
        """优化代码"""
        messages = [
            {"role": "system", "content": self.system_prompt},
            {"role": "user", "content": f"请优化这段代码:\n{code}"}
        ]
        return self._call_llm(messages)

def main():
    """主函数"""
    print("🚀 MiniCoder - 智能代码助手")
    print("=" * 50)
    
    # 检查API密钥
    if not API_KEY:
        print("⚠️  警告: 未设置 MODEL_KEY 环境变量")
        print("请创建 .env 文件并添加: MODEL_KEY=your_openai_key")
        print()
    
    coder = MiniCoder()
    
    # 简单的交互演示
    while True:
        print("\n请选择功能:")
        print("1. 生成代码")
        print("2. 解释代码")
        print("3. 修复bug")
        print("4. 优化代码")
        print("5. 退出")
        
        choice = input("\n请输入选项 (1-5): ").strip()
        
        if choice == "1":
            prompt = input("请描述需要生成的代码: ")
            language = input("编程语言 (默认python): ").strip() or "python"
            result = coder.generate_code(prompt, language)
            print(f"\n{result}")
        elif choice == "2":
            code = input("请输入要解释的代码: ")
            result = coder.explain_code(code)
            print(f"\n{result}")
        elif choice == "3":
            error = input("请输入错误信息: ")
            context = input("请输入代码上下文: ")
            result = coder.fix_bug(error, context)
            print(f"\n{result}")
        elif choice == "4":
            code = input("请输入要优化的代码: ")
            result = coder.optimize_code(code)
            print(f"\n{result}")
        elif choice == "5":
            print("👋 再见！")
            break
        else:
            print("❌ 无效选项，请重试")

if __name__ == "__main__":
    main()