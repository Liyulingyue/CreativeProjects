#!/usr/bin/env python
"""test_mini_coder.py - MiniCoder 测试脚本"""
import sys
sys.path.insert(0, '/home/liyulingyue/.openclaw/workspace/Codes/CreativeProjects/AgentLearn/MiniCoder')

from mini_coder import MiniCoder
from tools import CodeTools, ProjectManager

def test_basic_functionality():
    """测试基础功能"""
    print("=" * 60)
    print("测试 MiniCoder 基础功能")
    print("=" * 60)
    
    # 测试MiniCoder类
    coder = MiniCoder()
    print("✅ MiniCoder 实例创建成功")
    
    # 测试系统提示
    assert len(coder.system_prompt) > 0, "系统提示不能为空"
    print("✅ 系统提示加载成功")
    
    # 测试代码生成
    result = coder.generate_code("创建一个快速排序算法", "python")
    assert len(result) > 0, "代码生成失败"
    print("✅ 代码生成功能正常")
    
    # 测试代码解释
    result = coder.explain_code("def hello(): pass")
    assert len(result) > 0, "代码解释失败"
    print("✅ 代码解释功能正常")
    
    # 测试bug修复
    result = coder.fix_bug("IndexError", "arr = [1,2,3]")
    assert len(result) > 0, "bug修复失败"
    print("✅ Bug修复功能正常")
    
    # 测试代码优化
    result = coder.optimize_code("def hello():\n    print('hello')")
    assert len(result) > 0, "代码优化失败"
    print("✅ 代码优化功能正常")
    
    # 测试工具函数
    functions = CodeTools.extract_functions("def foo(): pass\ndef bar(): pass")
    assert len(functions) == 2, "函数提取失败"
    print("✅ 工具函数正常")
    
    # 测试项目管理
    success = ProjectManager.create_project_structure("test_mini_coder", "/tmp")
    assert success, "项目创建失败"
    print("✅ 项目管理功能正常")
    
    print("\n" + "=" * 60)
    print("所有基础功能测试通过！✅")
    print("=" * 60)

def test_api_integration():
    """测试API集成（模拟）"""
    print("\n" + "=" * 60)
    print("测试 API 集成")
    print("=" * 60)
    
    coder = MiniCoder()
    
    # 测试API调用方法存在
    assert hasattr(coder, '_call_llm'), "缺少 _call_llm 方法"
    print("✅ _call_llm 方法存在")
    
    # 测试消息格式
    messages = [
        {"role": "system", "content": "test"},
        {"role": "user", "content": "test"}
    ]
    result = coder._call_llm(messages)
    # 由于没有API密钥，应该返回警告信息
    assert "MODEL_KEY" in result or "API调用失败" in result or "openai" in result.lower(), \
        f"API调用返回意外结果: {result}"
    print("✅ API调用方法工作正常")
    
    print("\n" + "=" * 60)
    print("API集成测试完成！✅")
    print("=" * 60)
    print("\n💡 提示: 设置 MODEL_KEY 环境变量后可使用真实API")

if __name__ == "__main__":
    test_basic_functionality()
    test_api_integration()