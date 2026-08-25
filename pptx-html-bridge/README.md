# pptx-html-bridge

将PPTX文件转换为HTML的Python库。

## 安装

### 从源码安装

```bash
git clone https://github.com/Liyulingyue/pptx-html-bridge.git
cd pptx-html-bridge
pip install -e .
```

### 从PyPI安装 (WIP)
```bash
pip install pptx-html-bridge  # 尚未发布到PyPI
```

### 从GitHub安装

```bash
pip install git+https://github.com/Liyulingyue/pptx-html-bridge.git
```

## 使用

### 作为库

#### 使用类接口

```python
from pptx_html_bridge import PPTXToHTMLConverter

# 创建转换器实例
converter = PPTXToHTMLConverter(compact=True)

# 转换单个文件
result = converter.convert_file('presentation.pptx', 'output_dir')

# 转换目录中的所有PPTX文件
result = converter.convert_directory('pptx_files/', 'output_dir')
```

#### 使用便捷函数

```python
from pptx_html_bridge import convert_pptx_to_html

# 转换单个文件
result = convert_pptx_to_html('presentation.pptx', 'output_dir', compact=True)

# 转换目录
result = convert_pptx_to_html('pptx_files/', 'output_dir')
```

### 演示脚本

项目包含一个演示脚本 `demos/convert_demo.py`，展示了如何使用import方式调用库：

```bash
cd demos
python convert_demo.py
```

此脚本会：
- 自动清空输出目录
- 转换 `demos/source/test.pptx` 到 `demos/outputs/`
- 创建结构化的目录：
  - `slides/` - 存放所有HTML幻灯片文件
  - `media/` - 存放所有图片等媒体文件
  - 根目录 - 存放索引文件 `test_index.html`
- 显示详细的转换过程和结果

### 命令行

```bash
# 基本使用
pptx-to-html input.pptx --output output_dir

# 转换目录
pptx-to-html pptx_directory/ --output output_dir

# 紧凑输出（无换行）
pptx-to-html input.pptx --output output_dir --compact
```

## 输出结构

转换后的文件会按照以下结构组织：

```
output_directory/
├── slides/           # HTML幻灯片文件
│   ├── slide1.html
│   ├── slide2.html
│   └── ...
├── media/            # 图片、视频等媒体文件
│   ├── slide1_img0.jpg
│   ├── slide5_video1.mp4    # 视频文件
│   ├── slide5_poster1.png   # 视频海报帧
│   ├── master_img0.png
│   └── ...
└── [filename]_index.html  # 幻灯片索引页面
```

## 功能

- 将PPTX文件转换为HTML，每个幻灯片一个HTML文件
- 支持背景、字体、颜色等样式
- **增强的文本颜色提取**：正确处理PowerPoint中的自动颜色（白色文本等）
- **视频资源支持**：提取并嵌入PPTX中的视频文件，支持海报帧显示
- 生成导航索引页面
- 支持紧凑HTML输出
- 命令行和编程接口

## 依赖

- python-pptx
- lxml
