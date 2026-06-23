# PhotoAnalyzer

旅行相片自动分析工具 - 使用 OpenAI 多模态模型自动分析旅行照片质量、风格、内容等。

## 功能特性

- 支持单张图片和文件夹批量分析
- 输出结构化的 JSON/CSV 分析报告
- 支持多种图片格式 (jpg, png, gif, bmp, webp, tiff)
- 自定义分析维度和评分标准

## 安装

```bash
pip install -r requirements.txt
```

配置环境变量（或创建 `.env` 文件）：

```env
OPENAI_API_KEY=your-api-key-here
OPENAI_BASE_URL=https://api.minimaxi.com/v1
OPENAI_VISION_MODEL_NAME=MiniMax-M3
```

## 使用方法

### 分析单张图片

```bash
python analyze_single.py "path/to/image.jpg"
python analyze_single.py "path/to/image.jpg" -o result
python analyze_single.py "path/to/image.jpg" --csv
```

### 批量分析文件夹

```bash
python analyze_folder.py "path/to/folder"
python analyze_folder.py "path/to/folder" -o my_report
python analyze_folder.py "path/to/folder" --csv --delay 2.0
python analyze_folder.py "path/to/folder" --dry-run
```

### 参数说明

#### analyze_single.py

| 参数 | 说明 |
|------|------|
| `image_path` | 图片文件路径 |
| `-o, --output` | 输出文件路径（不含扩展名） |
| `--json` | 导出 JSON 格式 |
| `--csv` | 导出 CSV 格式 |
| `--no-export` | 不导出文件，只打印结果 |

#### analyze_folder.py

| 参数 | 说明 |
|------|------|
| `folder_path` | 文件夹路径 |
| `-o, --output` | 输出文件路径，默认 `analysis_result` |
| `-r, --recursive` | 递归遍历子文件夹（默认开启） |
| `--no-recursive` | 不递归遍历子文件夹 |
| `--json` | 导出 JSON 格式 |
| `--csv` | 导出 CSV 格式 |
| `--no-export` | 不导出文件，只打印摘要 |
| `--delay` | 每次请求间隔（秒），默认 1.0 |
| `--dry-run` | 仅列出将要分析的图片 |

## 输出格式

分析结果包含以下字段：

| 字段 | 说明 |
|------|------|
| `score` | 照片质量评分 (0-100) |
| `style` | 照片风格描述 |
| `caption` | 中文图片说明（不超过 30 字） |
| `main_objects` | 主要物体列表（至少 2 个） |
| `blurry` | 清晰度：模糊/略微模糊/清晰 |
| `comments` | 详细评价（至少 50 字） |
| `recommendations` | 改进建议（至少 30 字） |

## 项目结构

```
PhotoAnalyzer/
├── src/
│   ├── __init__.py      # 模块入口
│   ├── config.py        # 配置和常量
│   ├── analyzer.py      # 核心分析器
│   └── exporter.py      # 导出功能
├── analyze_single.py    # 单文件分析入口
├── analyze_folder.py    # 文件夹批量分析入口
├── test_real_client_img.py  # 测试脚本
├── requirements.txt
└── README.md
```

## 环境变量

| 变量 | 说明 | 默认值 |
|------|------|--------|
| `OPENAI_API_KEY` | OpenAI API 密钥 | `your-api-key-here` |
| `OPENAI_BASE_URL` | API 地址 | `https://api.minimaxi.com/v1` |
| `OPENAI_VISION_MODEL_NAME` | 视觉模型名称 | `MiniMax-M3` |

## License

MIT
