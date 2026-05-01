# 食谱卡片生成器

生成精美的食谱卡片模板，中心区域可放置您的美食照片，周围环绕食材清单和烹饪步骤。

## 功能特点

- **专业模板**：生成高质量食谱卡片模板
- **中心留白**：中心区域清晰空白，用于放置您的美食照片
- **多样样式**：支持现代、复古、优雅、活泼等设计风格
- **多菜系**：支持意大利、中国、墨西哥、日本等多种菜系
- **质量验证**：内置工具验证模板质量

## 快速开始

### 前置条件

1. Python 3.7+
2. 依赖包：`pip install openai Pillow numpy`
3. API 密钥：从 [Baidu AIStudio](https://aistudio.baidu.com/account/accessToken) 获取

### 环境配置（重要）

**请先检测虚拟环境！**

```bash
# 检测并激活虚拟环境（如果存在）
if [ -d ".venv" ]; then
    echo "检测到 .venv 虚拟环境，正在激活..."
    source .venv/bin/activate
fi

# 安装依赖
pip install openai Pillow numpy

# 设置 API 密钥
export BAIDU_API_KEY="your_api_key"

# 进入技能目录
cd recipe-card-generator
```

### 生成食谱卡片

```bash
# 生成意大利食谱卡片（现代风格）
python scripts/generate_recipe_template.py "Italian" \
    --style modern \
    --color-scheme warm \
    --output_dir output/

# 生成中国食谱卡片（复古风格）
python scripts/generate_recipe_template.py "Chinese" \
    --style rustic \
    --output_dir output/
```

### 验证生成质量

```bash
# 检查中心区域是否清晰
python scripts/validate_image.py output/italian_recipe_card.png --verbose
```

### 添加您的美食照片

1. 打开生成的食谱卡片图片
2. 将您的美食照片放置在中心区域
3. 添加菜名、食材清单和烹饪步骤文字
4. 保存并分享完成的食谱卡片

## 布局结构

```
+---------------------------+
|         菜名标题          |
|                           |
|   [步骤1 + 对应食材]      |
|         +-----+           |
| [步骤2] |     | [步骤3]   |
| +食材   | 美食 | +食材     |
|         | 照片 |          |
|   [步骤4 + 对应食材]      |
|         +-----+           |
|                           |
|    准备时间 |  份量        |
+---------------------------+
```

**核心特点**：
- 竖屏方向 (896x1200)
- 圆形中心区域用于放置美食照片
- 每个步骤与其对应的食材组合在一起
- 步骤+食材组合按照烹饪顺序顺时针排列
- 视觉流向遵循烹饪顺序

## 自定义选项

### 布局样式

| 样式 | 描述 |
|------|------|
| **circular** | 食材径向环绕中心排列（默认） |
| **grid** | 食材网格状环绕中心排列 |
| **sequential** | 食材顺序环绕中心排列 |

### 菜系类型

| 菜系 | 英文名称 |
|------|----------|
| 意大利 | Italian |
| 中国 | Chinese |
| 墨西哥 | Mexican |
| 日本 | Japanese |
| 印度 | Indian |
| 法国 | French |
| 泰国 | Thai |
| 地中海 | Mediterranean |

### 视觉样式

| 样式 | 描述 |
|------|------|
| **现代 (modern)** | 简洁线条，无衬线字体，极简主义 |
| **复古 (rustic)** | 复古纹理，手写字体，温暖色调 |
| **优雅 (elegant)** | 衬线字体，金色点缀，高端质感 |
| **活泼 (playful)** | 明亮色彩，圆润形状，趣味排版 |

### 配色方案

| 方案 | 描述 |
|------|------|
| **暖色 (warm)** | 大地色调，金色点缀，温馨氛围 |
| **冷色 (cool)** | 蓝绿色调，清新自然 |
| **中性 (neutral)** | 灰米色调，专业平衡 |
| **鲜艳 (vibrant)** | 明亮饱和，引人注目 |

### 图片尺寸

| 尺寸 | 比例 |
|------|------|
| 896x1200 | 竖屏（默认） |
| 768x1376 | 9:16 竖屏 |
| 848x1264 | 2:3 竖屏 |
| 1024x1024 | 正方形 |
| 1200x896 | 4:3 横屏 |
| 1264x848 | 3:2 横屏 |
| 1376x768 | 16:9 横屏 |

## 使用示例

### 基础食谱卡片

```bash
python scripts/generate_recipe_template.py "Italian" \
    --style modern \
    --output-dir output/
```

### 使用不同布局样式

```bash
# 环形布局（默认）- 食材径向环绕中心
python scripts/generate_recipe_template.py "Chinese" \
    --layout-style circular \
    --output-dir output/

# 网格布局 - 食材网格状环绕中心
python scripts/generate_recipe_template.py "Chinese" \
    --layout-style grid \
    --output-dir output/

# 顺序布局 - 食材顺序环绕中心
python scripts/generate_recipe_template.py "Chinese" \
    --layout-style sequential \
    --output-dir output/
```

### 高级食谱卡片

```bash
python scripts/generate_recipe_template.py "Japanese" \
    --style elegant \
    --color-scheme cool \
    --center-ratio 0.5 \
    --num-steps 8 \
    --layout-style circular \
    --custom-elements "营养信息" "烹饪技巧" \
    --output-dir output/
```

### 批量生成

```bash
# 生成多种菜系的食谱卡片
for cuisine in Italian Chinese Mexican Japanese; do
    python scripts/generate_recipe_template.py "$cuisine" \
        --style modern \
        --output-dir recipe_collection/
done
```

## 质量验证

### 验证内容

- **中心区域清晰度**：中心是否空白可用于放置照片
- **对比度**：文字是否清晰可读
- **构图平衡**：布局是否视觉平衡
- **宽高比**：卡片比例是否合适

### 质量检查清单

- [ ] 中心区域清晰可用于放置美食照片
- [ ] 文字清晰可读且位置合理
- [ ] 视觉风格符合预期
- [ ] 构图平衡协调
- [ ] 色彩和谐统一

## 目录结构

```
recipe-card-generator/
├── SKILL.md                    # 技能文档
├── README.md                   # 中文说明文档
├── README_en.md                # 英文说明文档
├── scripts/
│   ├── generate_image.py       # 核心图片生成脚本
│   ├── generate_recipe_template.py  # 食谱卡片模板生成器
│   ├── validate_image.py       # 图片验证脚本
│   └── test_skill.py           # 测试脚本
├── assets/
│   └── templates/              # 预设模板
└── references/
    ├── prompts.md              # 优化的提示词库
    └── styles.md               # 样式指南
```

## API 配置

### Ernie Image API

- **Base URL**: `https://aistudio.baidu.com/llm/lmapi/v3`
- **Model**: `ernie-image-turbo`
- **认证**: Baidu AIStudio API Key

### 支持的图片尺寸

| 尺寸 | 比例 |
|------|------|
| 1024x1024 | 正方形 |
| 1376x768 | 16:9 横屏 |
| 1264x848 | 3:2 横屏 |
| 1200x896 | 4:3 横屏 |
| 768x1376 | 9:16 竖屏 |
| 848x1264 | 2:3 竖屏 |
| 896x1200 | 3:4 竖屏 |

## 常见问题

### 中心区域不够清晰

```bash
# 方法1：增加中心区域比例
--center-ratio 0.5

# 方法2：在提示词中强调
"center area completely empty and clear"
```

### 文字不可读

在提示词中添加：
```
high contrast text, clear typography, readable fonts
```

### 样式不符合预期

```bash
# 使用更具体的样式描述
--style elegant --color-scheme warm
```

### API 认证失败

```bash
# 检查环境变量
echo $BAIDU_API_KEY

# 或显式传递
--api_key your_api_key
```

## 测试

```bash
# 运行测试脚本
python scripts/test_skill.py --verbose
```

## 完整示例

```bash
# 1. 设置 API 密钥
export BAIDU_API_KEY="your_api_key"

# 2. 生成意大利食谱卡片
python scripts/generate_recipe_template.py "Italian" \
    --style modern \
    --color-scheme warm \
    --output_dir output/

# 3. 验证生成质量
python scripts/validate_image.py output/italian_recipe_card.png --verbose

# 4. 在图片编辑软件中打开
# 5. 将您的美食照片放置在中心区域
# 6. 添加菜名、食材和步骤文字
# 7. 保存并分享
```

## 相关文档

- [SKILL.md](SKILL.md) - 完整技能文档
- [提示词库](references/prompts.md) - 优化的提示词模板
- [样式指南](references/styles.md) - 视觉样式详解

## 许可证

MIT License
