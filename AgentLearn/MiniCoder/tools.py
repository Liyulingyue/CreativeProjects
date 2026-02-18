#!/usr/bin/env python
"""tools.py - MiniCoder 工具函数"""
import os
import json
from pathlib import Path
from typing import List, Dict, Optional

class CodeTools:
    """代码相关工具类"""
    
    @staticmethod
    def read_file(file_path: str) -> str:
        """读取文件内容"""
        try:
            with open(file_path, 'r', encoding='utf-8') as f:
                return f.read()
        except Exception as e:
            return f"Error reading file: {e}"
    
    @staticmethod
    def write_file(file_path: str, content: str) -> bool:
        """写入文件"""
        try:
            with open(file_path, 'w', encoding='utf-8') as f:
                f.write(content)
            return True
        except Exception as e:
            print(f"Error writing file: {e}")
            return False
    
    @staticmethod
    def get_file_extension(file_path: str) -> str:
        """获取文件扩展名"""
        return Path(file_path).suffix
    
    @staticmethod
    def validate_python_syntax(code: str) -> bool:
        """验证Python语法"""
        try:
            compile(code, '<string>', 'exec')
            return True
        except SyntaxError as e:
            print(f"Syntax error: {e}")
            return False
    
    @staticmethod
    def format_code(code: str, language: str = "python") -> str:
        """格式化代码（简化版）"""
        # 这里可以集成black、autopep8等工具
        return code.strip()
    
    @staticmethod
    def extract_functions(code: str) -> List[str]:
        """提取函数名"""
        import re
        # 简单的正则匹配
        pattern = r'def\s+(\w+)\s*\('
        return re.findall(pattern, code)

class ProjectManager:
    """项目管理工具"""
    
    @staticmethod
    def create_project_structure(project_name: str, project_path: str = ".") -> bool:
        """创建项目基础结构"""
        try:
            base_path = Path(project_path) / project_name
            base_path.mkdir(exist_ok=True)
            
            # 创建基础目录
            for folder in ["src", "tests", "docs", "data"]:
                (base_path / folder).mkdir(exist_ok=True)
            
            # 创建基础文件
            (base_path / "README.md").touch()
            (base_path / "requirements.txt").touch()
            (base_path / ".gitignore").touch()
            
            print(f"✅ 项目 {project_name} 创建成功于 {base_path}")
            return True
        except Exception as e:
            print(f"❌ 创建项目失败: {e}")
            return False
    
    @staticmethod
    def get_project_files(project_path: str) -> Dict[str, str]:
        """获取项目中的所有文件"""
        files = {}
        try:
            for root, dirs, filenames in os.walk(project_path):
                for filename in filenames:
                    full_path = os.path.join(root, filename)
                    rel_path = os.path.relpath(full_path, project_path)
                    files[rel_path] = full_path
        except Exception as e:
            print(f"Error scanning files: {e}")
        return files

# 便捷函数
def print_success(message: str):
    """打印成功消息"""
    print(f"✅ {message}")

def print_error(message: str):
    """打印错误消息"""
    print(f"❌ {message}")

def print_info(message: str):
    """打印信息消息"""
    print(f"!{message}")

if __name__ == "__main__":
    # 测试工具函数
    print("Testing MiniCoder tools...")
    
    # 测试文件操作
    test_code = "def hello():\n    print('Hello World')"
    print(f"Extracted functions: {CodeTools.extract_functions(test_code)}")
    
    # 测试项目创建
    ProjectManager.create_project_structure("test_project", "/tmp")
    
    print("✅ All tools loaded successfully!")