# SimpleFileManager 架构设计

## 概述

SimpleFileManager 是一个轻量级的本地文件管理 Web 应用，采用前后端分离架构，后端使用 Python FastAPI，前端使用 React + TypeScript。

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
│  ┌────────────┐  ┌────────────┐  ┌────────────────────┐      │
│  │FileService│  │IndexService│ │  SearchService    │      │
│  └────────────┘  └────────────┘  └────────────────────┘      │
├─────────────────────────┼───────────────────────────────────┤
│                   Storage Layer                             │
│  ┌────────────┐  ┌────────────┐  ┌────────────────────┐      │
│  │  SQLite    │  │  LanceDB   │  │   File System     │      │
│  │ (metadata) │  │ (vectors)  │  │   (actual files)  │      │
│  └────────────┘  └────────────┘  └────────────────────┘      │
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
| `search` | `/api/search` | 搜索接口 |
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

#### IndexService
- `scan_root()` - 扫描根目录
- `incremental_update()` - 增量更新索引
- `get_stats()` - 获取索引统计

#### SearchService
- `search(query)` - 全文搜索
- `semantic_search(query)` - 向量语义搜索
- `suggest(query)` - 搜索建议

### 3. Storage Layer

#### SQLite (元数据)
- 文件索引表
- 设置表
- 任务计划表

#### LanceDB (向量)
- 文件内容向量
- 搜索结果缓存

### 4. Model Layer

与外部 LLM 服务通信：

- 文本嵌入 (Embedding)
- 内容摘要 (Summarization)
- 文件分类 (Classification)

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
    "extension": str,       # 文件扩展名
    "mime_type": str,      # MIME类型
}
```

### FileIndex (SQLite)
```sql
CREATE TABLE file_index (
    id INTEGER PRIMARY KEY,
    path TEXT UNIQUE NOT NULL,
    name TEXT NOT NULL,
    parent_path TEXT,
    is_dir BOOLEAN,
    size INTEGER,
    modified_at TEXT,
    indexed_at TEXT,
    file_hash TEXT,
    metadata JSON
);
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

### 分页加载
- 大目录使用分页加载
- 前端虚拟列表优化

### 缓存策略
- 目录结构缓存
- 缩略图缓存
- 搜索结果缓存

## 后续演进

### V0.3 - 向量化
- 集成 LanceDB
- 实现文本嵌入
- 语义搜索

### V0.4 - AI 规划
- 文件内容分析
- 自动归类建议
- 审批工作流

### Rust 迁移
- 性能敏感模块用 Rust 重写
- 减小二进制体积
- 提升并发能力
