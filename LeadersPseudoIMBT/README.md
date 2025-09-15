# LeaderMIBT - 领导特性调研与分析工具

## 项目简介

LeaderMIBT 是一个基于 Python 和 Gradio 的领导特性调研工具。通过一组精心设计的调研问题，分析领导的各种特性倾向，并生成可视化的雷达图和柱状图。同时，基于分析结果提供个性化的交互建议，帮助用户更好地与领导相处。

## 主要功能

- **调研模块**: 通过30个问题调研领导的特性（加班看法、语言使用、对话主导等）
- **分析模块**: 生成特性倾向的雷达图和柱状图
- **建议模块**: 基于分析结果提供交互建议
- **可视化界面**: 使用 Gradio 提供友好的 Web 界面

## 安装说明

1. 克隆项目到本地：
```bash
git clone <repository-url>
cd LeaderMIBT
```

2. 创建虚拟环境（如果不存在）：
```bash
python -m venv .venv
```

3. 激活虚拟环境：
```bash
.\.venv\Scripts\activate  # Windows
```

4. 安装依赖（使用清华源）：
```bash
pip install -r requirements.txt -i https://pypi.tuna.tsinghua.edu.cn/simple
```

## 使用方法

运行主程序：
```bash
python start.py
```

或者直接运行：
```bash
python main.gradio.py
```

在浏览器中打开显示的地址（通常是 http://127.0.0.1:7860），开始使用工具。

## 项目结构

```
LeaderMIBT/
├── README.md                    # 项目说明文档
├── requirements.txt            # 依赖文件
├── start.py                    # 启动脚本
├── main.gradio.py             # 主界面文件
├── config/
│   └── questions.json         # 调研问题配置文件
├── modules/
│   ├── survey.py              # 调研模块
│   ├── analysis.py            # 分析模块
│   └── advice.py              # 建议模块
└── charts/                    # 图表文件目录
    ├── radar_chart.png        # 雷达图
    └── bar_chart.png          # 柱状图
```

## 技术栈

- Python 3.8+
- Gradio (Web界面)
- Matplotlib (图表生成)
- JSON (配置管理)

## 注意事项

- 确保使用UTF-8编码保存所有Python文件
- 如果遇到编码问题，请检查文件编码设置
- 建议使用虚拟环境运行项目
- 程序将在 http://0.0.0.0:7860 上运行
- **已修复**: 之前的TypeError问题已解决，现在可以正确处理用户选择的选项
- **已修复**: Windows路径长度限制问题已解决，现在使用文件路径而不是base64数据URI

## 许可证

MIT License