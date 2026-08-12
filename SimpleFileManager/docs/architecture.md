# SimpleFileManager 架构设计

## 概述

SimpleFileManager 是一个轻量级的本地文件管理 Web 应用，采用前后端分离架构：
- 后端：Python FastAPI
- 前端：React + TypeScript + Vite
- 向量数据库：LanceDB
- 元数据存储：SQLite (通过 LanceDB)

## 系统架构

```
┌─────────────────────────────────────────────────────────────┐
│                        Client                               │
│                    (Browser / Web)                          │
└─────────────────────────┬───────────────────────────────────┘
                          │ HTTP/REST
┌─────────────────────────▼───────────────────────────────────┐
│                      API Layer                              │
│                   (FastAPI / Uvicorn)                       │
├─────────────────────────┬───────────────────────────────────┤
│                   Service Layer                             │
│  ┌────────────┐  ┌────────────┐  ┌────────────────────┐   │
│  │FileService │  │IndexService│  │   RAGService      │   │
│  └────────────┘  └────────────┘  └────────────────────┘   │
├─────────────────────────┼───────────────────────────────────┤
│                   Storage Layer                             │
│  ┌────────────┐  ┌────────────┐  ┌────────────────────┐   │
│  │  SQLite    │  │  LanceDB   │  │   File System     │   │
│  │(settings)  │  │ (vectors)  │  │   (user files)   │   │
│  └────────────┘  └────────────┘  └────────────────────┘   │
└─────────────────────────────────────────────────────────────┘
                          │
┌─────────────────────────▼───────────────────────────────────┐
│                  External Services                           │
│              (OpenAI Compatible API)                         │
│                 (Ollama / vLLM / etc)                        │
└─────────────────────────────────────────────────────────────┘
```

## 模块设计

### 1. API Layer (routers)

负责 HTTP 请求处理，输入验证，输出格式化。

| Router | 路径 | 说明 |
|--------|------|------|
| `fs` | `/api/fs` | 文件系统操作：浏览、创建、删除、移动等 |
| `search` | `/api/search` | 文件名搜索 |
| `rag` | `/api/rag` | RAG 问答 |
| `settings` | `/api/settings` | 应用设置 |

### 2. Service Layer

业务逻辑处理层。

#### FileService
- `browse(path)` - 浏览目录
- `create_folder(path, name)` - 创建文件夹
- `delete(path)` - 删除文件/文件夹
- `move(src, dest)` - 移动文件
- `copy(src, dest)` - 复制文件
- `get_file_info(path)` - 获取文件信息

#### RAGService
- `index_file(file_path, content)` - 索引单个文件
- `index_files(files)` - 批量索引文件
- `query(question, top_k)` - 问答查询
- `clear_index()` - 清空索引

#### EmbeddingService
- `embed(texts)` - 文本向量化

#### LanceDBVectorStore
- `add(vector, metadata)` - 添加向量
- `search(query_vector, top_k)` - 向量搜索
- `delete_by_file(file_path)` - 删除文件关联的向量
- `clear()` - 清空向量库

### 3. Storage Layer

#### 目录结构
```
STORAGE_PATH/                    # .env 中 STORAGE_PATH 配置的目录
├── .simplefilemanager/          # 程序数据目录
│   ├── vectors.lance           # LanceDB 向量数据库
│   ├── metadata.db             # SQLite 元数据
│   ├── settings.json           # 应用设置
│   └── index_stats.json        # 索引统计
├── photos/
├── documents/
└── videos/
```

#### SQLite (元数据)
- 应用设置 (settings.json)
- 索引统计 (index_stats.json)

#### LanceDB (向量)
- 表名: `file_embeddings`
- 字段: `id`, `file_path`, `content`, `vector`
- 持久化存储在 `STORAGE_PATH/.simplefilemanager/vectors.lance`

### 4. Model Layer

与外部 LLM/Embedding 服务通信：

- **Embedding**: 文本嵌入 (text-embedding-3-small 等)
- **LLM**: 对话生成 (gpt-4o-mini 等)

## 数据模型

### FileNode
```python
{
    "name": str,           # 文件名
    "path": str,           # 完整路径
    "is_dir": bool,        # 是否为目录
    "size": int,           # 文件大小(字节)
    "modified": str,       # 修改时间 ISO格式
    "created": str,        # 创建时间 ISO格式
    "extension": str,      # 文件扩展名
    "mime_type": str,      # MIME类型
}
```

### AppSettings
```python
{
    "llm_api_key": str,
    "llm_base_url": str,
    "llm_model": str,
    "embedding_api_key": str,
    "embedding_base_url": str,
    "embedding_model": str,
    "embedding_dim": str,
    "index_interval": int,
    "storage_path": str,
    "theme": str,
}
```

## 安全考虑

### 文件操作安全
1. 路径验证 - 防止路径遍历攻击 (`../`)
2. 操作审计 - 记录所有文件操作日志
3. 权限检查 - 验证用户权限

### Human-in-the-Loop
所有破坏性操作（删除、移动）需要用户确认。

## 性能优化

### 增量索引
- 记录上次索引时间
- 只扫描变更的文件
- 使用 mtime + size 快速检测变更

### 向量检索
- LanceDB 自动处理 ANN 索引
- 余弦相似度计算

## 后续演进

### V0.4 - AI 规划
- 文件内容分析
- 自动归类建议
- 审批工作流

### Rust 迁移
- 性能敏感模块用 Rust 重写
- 减小二进制体积
- 提升并发能力

## 数据迁移

### 从旧版本迁移 (v0.3 → v0.4)

旧版本数据位于 `backend/data/`：
```
backend/data/
├── vectors.lance
├── metadata.db
├── settings.json
└── index_stats.json
```

迁移到新结构 `STORAGE_PATH/.simplefilemanager/`：

```bash
# 1. 停止服务

# 2. 确保 .env 中配置了 STORAGE_PATH（如 STORAGE_PATH=./data）

# 3. 迁移数据文件
mkdir -p data/.simplefilemanager
mv backend/data/vectors.lance data/.simplefilemanager/
mv backend/data/metadata.db data/.simplefilemanager/
mv backend/data/settings.json data/.simplefilemanager/
mv backend/data/index_stats.json data/.simplefilemanager/

# 4. 验证
ls -la data/.simplefilemanager/

# 5. 可选：删除旧数据目录
rm -rf backend/data/
```

## 数据清理

### 清理向量索引（保留设置）

当索引损坏或需要重新生成时：

```bash
rm -rf data/.simplefilemanager/vectors.lance
rm -rf data/.simplefilemanager/index_stats.json
# 重启后会自动重建索引
```

### 清理所有程序数据（恢复出厂设置）

```bash
rm -rf data/.simplefilemanager/
# 重启后会自动创建新目录
```

### 注意事项

- `.simplefilemanager/` 目录包含所有程序数据，删除后需要重新索引
- 用户文件（photos/、documents/ 等）不受影响
- 清理前建议备份设置文件 `settings.json`
