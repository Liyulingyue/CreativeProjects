#!/usr/bin/env python3
"""
演示脚本：使用import方式调用pptx-html-bridge库进行PPTX到HTML转换

此脚本展示了如何：
1. 导入pptx_html_bridge库
2. 清空输出目录
3. 转换PPTX文件到HTML
"""

import os
import shutil
from pptx_html_bridge import PPTXToHTMLConverter

def main():
    """主函数：演示PPTX到HTML的转换过程"""

    # 获取脚本所在目录的父目录（项目根目录）
    script_dir = os.path.dirname(os.path.abspath(__file__))
    project_dir = os.path.dirname(script_dir)

    # 定义路径
    source_file = os.path.join(project_dir, "demos", "source", "test.pptx")
    output_dir = os.path.join(project_dir, "demos", "outputs")

    print("=== PPTX to HTML 转换演示 ===\n")

    # 检查源文件是否存在
    if not os.path.exists(source_file):
        print(f"❌ 错误：源文件不存在 - {source_file}")
        return 1

    print(f"📁 源文件：{source_file}")
    print(f"📁 输出目录：{output_dir}")

    # 步骤1：清空输出目录
    print("\n🧹 步骤1：清空输出目录...")
    if os.path.exists(output_dir):
        shutil.rmtree(output_dir)
        print(f"   ✓ 已删除旧的输出目录：{output_dir}")
    else:
        print(f"   ℹ 输出目录不存在，跳过删除")

    # 步骤2：创建输出目录
    print("\n📂 步骤2：创建输出目录...")
    os.makedirs(output_dir, exist_ok=True)
    print(f"   ✓ 已创建输出目录：{output_dir}")

    # 步骤3：初始化转换器并进行转换
    print("\n🔄 步骤3：初始化转换器...")
    converter = PPTXToHTMLConverter(compact=True)
    print("   ✓ 转换器初始化完成")

    print("\n🚀 步骤4：开始转换...")
    try:
        result = converter.convert_file(source_file, output_dir)

        print("   ✓ 转换完成！")
        print(f"   📊 转换结果：")
        print(f"      - PPTX文件：{result['pptx_file']}")
        print(f"      - 输出目录：{result['output_dir']}")
        print(f"      - 生成文件数：{len(result['generated_files'])}")

        # 显示生成的文件列表
        print(f"      - 生成的文件：")
        for file_path in result['generated_files']:
            print(f"        • {file_path}")

        print("\n🎉 转换成功完成！")
        return 0

    except Exception as e:
        print(f"❌ 转换失败：{e}")
        return 1

if __name__ == "__main__":
    exit(main())