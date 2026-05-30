# LLMLabel

基于大模型标注 + 小模型蒸馏的半自动化数据分类流水线，支持多种任务类型。

## 支持的任务类型

| 任务 | 大模型标注 | 小模型训练 | 小模型推理 |
|------|-----------|-----------|-----------|
| 文本分类 `text` | GPT-4o + openaijsonwrapper | Chinese-BERT / RoBERTa | 微调后的 BERT |
| 图片分类 `image` | GPT-4V (vision) | Vision Transformer (ViT) | 微调后的 ViT |

## 方案设计

```
┌─────────────────────────────────────────────┐
│  1. 大模型打样 (Seed Generation)            │
│     从原始数据中抽取部分样本                 │
│     用大模型 + openaijsonwrapper 标注        │
│     ↓                                      │
│  2. 小模型训练 (Model Fine-tuning)          │
│     用标注数据微调小模型                   │
│     ↓                                      │
│  3. 小模型推理 (Inference)                 │
│     微调后的小模型接管全量分类任务           │
└─────────────────────────────────────────────┘
```

## 快速开始

### 安装依赖

```bash
pip install -r requirements.txt
```

### 配置环境变量

```bash
cp .env.example .env
# 编辑 .env，填入 MODEL_KEY 等
```

### 阶段一：大模型标注

**文本分类：**
```bash
python scripts/run_pipeline.py \
    --stage 1 --task text \
    --raw-data data/raw/texts.jsonl \
    --labeled-output data/labeled/labeled.jsonl \
    --sample-size 10000 \
    --categories "正例,负例" \
    --instruction "判断该文本是否与医疗相关"
```

**图片分类：**
```bash
python scripts/run_pipeline.py \
    --stage 1 --task image \
    --raw-data data/raw/images/ \
    --labeled-output data/labeled/labeled.jsonl \
    --sample-size 10000 \
    --categories "医疗相关,非医疗相关" \
    --instruction "判断这张图片是否与医疗相关"
```

### 阶段二：模型训练

**文本分类（BERT）：**
```bash
python scripts/run_pipeline.py \
    --stage 2 --task text \
    --labeled-output data/labeled/labeled.jsonl \
    --bert-model hfl/chinese-roberta-wwm-ext \
    --num-epochs 3 --batch-size 16
```

**图片分类（ViT）：**
```bash
python scripts/run_pipeline.py \
    --stage 2 --task image \
    --labeled-output data/labeled/labeled.jsonl \
    --image-model google/vit-base-patch16-224 \
    --num-epochs 3 --batch-size 16
```

### 阶段三：小模型推理

```bash
# 文本
python scripts/run_pipeline.py \
    --stage 3 --task text \
    --raw-data data/raw/texts.jsonl \
    --model-output data/output/model/final \
    --final-output data/output/classified.jsonl \
    --device cuda

# 图片
python scripts/run_pipeline.py \
    --stage 3 --task image \
    --raw-data data/raw/images/ \
    --model-output data/output/model/final \
    --final-output data/output/classified.jsonl \
    --device cuda
```

## 文件结构

```
LLMLabel/
├── README.md
├── requirements.txt
├── .env.example
├── .gitignore
├── src/
│   ├── __init__.py
│   ├── task_type.py        # 任务类型枚举
│   ├── base.py             # 基类（LabelItem, BaseLabeler, BaseTrainer, BaseClassifier）
│   ├── text_labeler.py     # 文本标注器（GPT-4o）
│   ├── text_trainer.py     # 文本训练器（BERT）
│   ├── text_classifier.py  # 文本分类器
│   ├── image_labeler.py    # 图片标注器（GPT-4V）
│   ├── image_trainer.py    # 图片训练器（ViT）
│   ├── image_classifier.py # 图片分类器
│   └── utils.py            # 工具函数
├── data/
│   ├── raw/                # 原始数据
│   ├── labeled/            # 标注数据
│   └── output/             # 模型输出、分类结果
└── scripts/
    └── run_pipeline.py     # 流水线入口
```

## 数据格式

### 输入

**文本**（`.jsonl`，每行含 `text` 字段）：
```json
{"text": "这是一段待分类的文本内容"}
```

**图片**（支持两种方式）：
- 目录：直接传入图片文件夹路径，支持 `.jpg/.png/.bmp/.webp`
- `.jsonl`：每行含 `image_path` 字段：
```json
{"image_path": "data/raw/images/photo001.jpg"}
```

### 标注输出（`.jsonl`）

```json
{"id": "llm_0", "text": "文本内容或图片路径", "label": "正例", "confidence": null, "source": "llm"}
{"id": "sm_1", "text": "文本内容或图片路径", "label": "负例", "confidence": 0.9876, "source": "small_model"}
```

## 模型推荐

### 文本分类

| 模型 | 说明 |
|------|------|
| `hfl/chinese-roberta-wwm-ext` | 中文 RoBERTa，最常用 |
| `hfl/chinese-macbert-base` | MacBERT，中文效果好 |
| `bert-base-chinese` | 原版 BERT 中文 |
| `hfl/chinese-electra-180g-base-discriminator` | ELECTRA 结构 |

### 图片分类

| 模型 | 说明 |
|------|------|
| `google/vit-base-patch16-224` | ViT 标准版，效果好 |
| `microsoft/resnet-50` | ResNet，稳定可靠 |
| `facebook/deit-small-patch16-224` | DeiT，轻量版 |

## 环境变量

```env
MODEL_KEY=your-api-key
MODEL_URL=https://api.openai.com/v1
MODEL_NAME=gpt-4o
```
