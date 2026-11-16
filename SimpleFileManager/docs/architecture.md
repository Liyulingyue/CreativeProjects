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

#### 上传：先收齐，再放好

大文件不是一个请求传完的。一份文件分几片送来，服务器先把每一片
放在暂存区（`STORAGE_PATH/.uploads/`），最后一片到了才拼成一个文件
放到目标位置。

- 暂存区在存储根目录下的点目录里：文件浏览器看不见它，索引跳过它，
  备份也跳过它。
- 断掉的连接可以接着传：`/upload/init` 会把**同一个文件**（同名、同
  大小、同一个人、同一个指纹）上一次还没传完的那份交回来，客户端
  只补缺的那几片。指纹对不上就重新开始——宁可重传，也不能把两个人的
  片子拼成一个文件。
- 每一片的大小上限 16 MB（`MAX_CHUNK_BYTES`）；超了直接拒绝，不读进
  内存里再说。
- 目标位置已经有同名**文件夹**时拒绝：不能用文件把文件夹顶掉。

#### 备份

`scripts/backup.py` 把整个存储根目录打包成一个 tar.gz。里面的东西分
两类：用户的文件（丢了就没了）和让它可搜的索引（能重建，但很贵）。

- **数据库用快照，不直接拷。** 服务正开着它们，从活着的 SQLite 底下
  拷一份出来，得到的归档可能根本打不开；备份 API 给的是一致的快照。
- **向量库是索引器正在写的目录**，直接拷会拷到半个数据文件，归档里的
  向量库就打不开了。所以备份先让服务把索引停下来，等手头那个文件写
  完，再拷，拷完让索引接着跑（`SFM_PASSWORD` 要给上）。
- 点目录（除了 `.simplefilemanager`）不进归档：挂载点、半截的下载、
  编辑器的临时文件。

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
    # LLM credentials live in the llm_providers table (CRUD at
    # /api/providers, rendered in /settings). AppSettings no longer
    # carries them — see "LLM Providers" below.
    "embedding_api_key": str,
    "embedding_base_url": str,
    "embedding_model": str,
    "embedding_dim": str,
    # Search-time embedding override; falls back to the main
    # embedding_* when unset.
    "search_api_key": str,
    "search_base_url": str,
    "search_model": str,
    "index_interval": int,
    "storage_path": str,
    "theme": str,
    "thinking_enabled": bool,  # LFM scratchpad; restart to apply
}
```

### LLM Providers
```python
# Each row in llm_providers is one configured LLM endpoint.
{
    "id": str,            # uuid hex
    "name": str,          # display only
    "base_url": str,      # OpenAI-compatible
    "model": str,
    "api_key": str,
    "is_main": bool,      # singleton; new sessions default to it
    "created_at": int,    # ms epoch
    "updated_at": int,
}
# Resolution precedence for an agent turn:
#   1. AgentRequest.provider_id (per-turn override from the chat
#      dropdown) — stale/bogus ids fall through.
#   2. chat_sessions.provider_id (sticky default for the session;
#      updated on every successful override).
#   3. The system main (is_main=1). Falls back to an error when
#      no main is configured — the operator must add one in
#      /settings before chat can be used.
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
