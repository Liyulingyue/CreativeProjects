# MiniRAG

一个基于 MiniClaudeCode 的简单检索增强生成 (RAG) 代理。

## 功能

- 具有 RAG 能力的交互式 CLI 聊天
- 关键词搜索 + 语义搜索（向量化检索）
- 文件名和内容双重匹配
- 子代理支持复杂任务
- FastAPI 服务器用于 API 访问

## 设置

1. 安装依赖：
   ```bash
   pip install -r requirements.txt
   ```

2. 在 `.env` 中设置环境变量：
   ```
   MODEL_KEY=your_api_key
   MODEL_URL=your_base_url  # 可选
   MODEL_NAME=your_model_name  # 可选，默认 gpt-4
   ```

## 使用

### CLI 模式

运行交互式聊天：
```bash
python my_rag.py
```

或运行单个查询：
```bash
python my_rag.py "your query here"
```

### Gradio Web 界面
启动美观的 Web 聊天界面：
```bash
python gradio_app.py
```

然后在浏览器中访问 `http://localhost:7860`

**界面特性**：
- 💬 实时对话
- 📚 示例问题
- 🧹 一键清空对话
- 🎨 现代化 UI

## 知识库设置

将你的知识文档放入 `knowledge/` 文件夹中。检索时只会搜索这个文件夹的内容。

文件夹结构：
```
MiniRAG/
├── knowledge/          # 知识文档存放处
│   └── .gitkeep       # 保持文件夹存在
├── .gitignore         # 忽略knowledge内容但保留.gitkeep
└── ...
```

你可以放入各种格式的文档：.txt, .md, .pdf, .docx 等（grep支持文本搜索）。

## 智能检索策略

MiniRAG 采用智能检索策略来提供准确答案：

### 检索决策
- **评估需求**：判断问题是否需要知识库检索
- **工具选择**：
  - `semantic_search`：概念性问题（如"如何训练模型"）
  - `search_files`：具体技术术语或代码查询
  - `search_filenames`：按文档名称查找

### 迭代搜索
- **关键词优化**：如果首次搜索失败，尝试同义词或相关概念
- **最大尝试**：每个问题最多尝试3次搜索
- **智能停止**：3次失败后确认知识库无相关信息
- **策略说明**：每次重试前解释新的搜索策略

### 示例策略
- 问题："梯度下降如何工作？" → `semantic_search("梯度下降算法")`
- 问题："查找API文档" → `search_filenames("api")` + `search_files("文档")`
- 问题："模型X的超参数是什么？" → `search_files("超参数")` + `semantic_search("模型配置")`

## 性能说明

### 首次语义搜索初始化
使用 `semantic_search` 时，系统会进行一次性初始化：
- 🔄 加载语义模型 (~23MB)
- 📄 处理知识库文档
- 🔄 生成文本嵌入向量
- 🔄 构建搜索索引

**预计时间**：根据文档数量，首次搜索可能需要10-60秒。后续搜索瞬间完成。

### 预构建索引（推荐）
为了避免首次搜索的延迟，可以预先构建语义索引：

```bash
python search_utils.py
```

这会生成以下文件：
- `semantic_index.faiss` - 向量索引
- `documents.pkl` - 文档块数据
- `file_paths.pkl` - 文件路径映射

**预构建后**，语义搜索会立即可用，无需等待初始化。

### 自动保存
系统会在首次构建索引后自动保存，下次运行时自动加载。