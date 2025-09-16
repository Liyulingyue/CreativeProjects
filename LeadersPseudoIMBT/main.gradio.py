# -*- coding: utf-8 -*-
"""
领导特性调研工具主入口
重构后的简洁版本，功能模块化到子文件夹中
"""

from ui.interface import create_interface

if __name__ == "__main__":
    print("Starting Leader Characteristics Survey Tool...")
    interface = create_interface()
    print("Interface created successfully. Launching server...")
    interface.launch(server_name="0.0.0.0", server_port=7861, show_error=True, share=False)
