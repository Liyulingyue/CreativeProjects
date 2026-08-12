# SimpleFileManager

**NASMind 的前身** - 一个轻量级的智能文件管理助手。

## 愿景

让每个拥有树莓派或 N100 迷你主机的用户，都能拥有一套完整的本地 AI 文件管理方案。

- **轻量**: 单二进制或 Docker 部署，占用资源少
- **智能**: 基于本地 LLM 的文件分析、归类和检索
- **安全**: 所有数据本地处理，不上云
- **可扩展**: 模块化设计，支持插件和自定义规则

## 核心功能

### 1. 文件浏览与管理
- [x] 目录树导航
- [x] 文件预览（图片、文本、PDF 等）
- [x] 批量操作（移动、复制、删除）
- [ ] 收藏和标签

### 2. 智能索引
- [x] 自动扫描和索引文件系统
- [x] 增量更新，监控变动
- [x] 支持多种文件类型识别

### 3. 向量化搜索
- [x] 文件内容向量化 (LanceDB)
- [x] 自然语言搜索
- [x] 语义相似度匹配

### 4. AI 问答 (RAG)
- [x] 基于文件内容的问答
- [x] 参考文档溯源
- [ ] 自动文件索引

### 5. AI 文件规划 (规划中)
- 自动分析文件结构和内容
- 提出归档建议
- 审批机制确保安全

### 6. Agent 执行助手 (规划中)
- 复杂文件操作自动化
- React Tools 调度

## 技术架构

```
┌─────────────────────────────────────────────────────────────┐
│                     SimpleFileManager                        │
├───────────────────────────┬─────────────────────────────────┤
│        Frontend           │           Backend               │
│   (vite + React + TS)    │        (FastAPI / Rust)         │
├───────────────────────────┴─────────────────────────────────┤
│                      Service Layer                           │
│  ┌─────────────┐ ┌─────────────┐ ┌─────────────────────┐    │
│  │ FileService │ │IndexService │ │   RAGService       │    │
│  └─────────────┘ └─────────────┘ └─────────────────────┘    │
├─────────────────────────────────────────────────────────────┤
│                      Storage Layer                           │
│  ┌─────────────┐  ┌─────────────┐  ┌───────────────────┐    │
│  │  SQLite     │  │  LanceDB    │  │   File System     │    │
│  │ (metadata)  │  │ (vectors)   │  │                   │    │
│  └─────────────┘  └─────────────┘  └───────────────────┘    │
├─────────────────────────────────────────────────────────────┤
│                      Model Layer                             │
│  ┌─────────────────────────────────────────────────────────┐│
│  │              OpenAI Compatible API                       ││
│  │         (Ollama / vLLM / llama.cpp)                     ││
│  └─────────────────────────────────────────────────────────┘│
└─────────────────────────────────────────────────────────────┘
```

## 快速开始

### 后端

```bash
cd backend
python -m venv .venv
.venv\Scripts\Activate.ps1  # Windows
# source .venv/bin/activate  # Linux/Mac
pip install -r requirements.txt
python run.py
```

### 前端

```bash
cd frontend
npm install
npm run dev
```

### 配置

复制 `.env.example` 为 `.env` 并配置：

```env
# LLM 配置 (对话/问答)
LLM_API_KEY=your-api-key-here
LLM_BASE_URL=https://api.minimaxi.com/v1/chat/completions
LLM_MODEL=gpt-4o-mini

# Embedding 配置 (向量化)
EMBEDDING_API_KEY=your-api-key-here
EMBEDDING_BASE_URL=https://api.minimaxi.com/v1/embeddings
EMBEDDING_MODEL=text-embedding-3-small
EMBEDDING_DIM=AUTO

# 索引配置
INDEX_INTERVAL=300
STORAGE_PATH=./data          # 被监控的根目录（程序数据将放在此目录的 .simplefilemanager 子目录中）
```

### 目录结构

```
STORAGE_PATH/                    # .env 中配置的 STORAGE_PATH
├── .simplefilemanager/          # 程序数据（自动创建）
│   ├── vectors.lance           # 向量数据库
│   ├── metadata.db             # SQLite 元数据
│   ├── settings.json           # 应用设置
│   └── index_stats.json        # 索引统计
├── photos/
├── documents/
└── videos/
```

## 数据迁移与清理

如果从旧版本升级，数据目录从 `./data/` 迁移到 `./data/.simplefilemanager/`：

```bash
# 1. 停止服务

# 2. 迁移数据（如果之前有数据）
mv data/vectors.lance data/.simplefilemanager/
mv data/metadata.db data/.simplefilemanager/
mv data/settings.json data/.simplefilemanager/
mv data/index_stats.json data/.simplefilemanager/

# 3. 验证
ls data/.simplefilemanager/
```

### 数据清理

清理程序数据（重置索引和设置）：

```bash
# 方式一：删除程序数据目录（保留用户文件）
rm -rf data/.simplefilemanager/

# 方式二：只清理向量索引（保留设置）
rm -rf data/.simplefilemanager/vectors.lance
rm -rf data/.simplefilemanager/index_stats.json

# 方式三：清理所有（恢复出厂设置）
rm -rf data/.simplefilemanager/
# 重启后会自动创建新目录
```

## API 接口

### 文件管理
| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/api/fs/browse` | 浏览目录 |
| GET | `/api/fs/tree` | 获取目录树 |
| POST | `/api/fs/create_folder` | 创建文件夹 |
| POST | `/api/fs/move` | 移动文件 |
| POST | `/api/fs/copy` | 复制文件 |
| POST | `/api/fs/delete` | 删除文件 |

### 搜索
| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/api/search/query` | 文件名搜索 |
| GET | `/api/search/suggest` | 搜索建议 |

### RAG 问答
| 方法 | 路径 | 说明 |
|------|------|------|
| POST | `/api/rag/index` | 索引单个文件 |
| POST | `/api/rag/index_batch` | 批量索引文件 |
| POST | `/api/rag/query` | 问答查询 |
| GET | `/api/rag/status` | 索引状态 |
| DELETE | `/api/rag/clear` | 清空索引 |

### 设置
| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/api/settings` | 获取设置 |
| POST | `/api/settings` | 更新设置 |
| GET | `/api/settings/index_stats` | 索引统计 |

## 项目结构

```
SimpleFileManager/
├── docs/                    # 设计文档
│   └── architecture.md
├── backend/                 # 后端服务
│   ├── app/
│   │   ├── __init__.py
│   │   ├── main.py         # FastAPI 入口
│   │   ├── models.py       # Pydantic 模型
│   │   ├── deps.py         # 依赖注入/状态管理
│   │   └── routers/        # API 路由
│   │       ├── __init__.py
│   │       ├── fs.py       # 文件系统路由
│   │       ├── search.py   # 搜索路由
│   │       ├── rag.py      # RAG 问答路由
│   │       └── settings.py # 设置路由
│   ├── requirements.txt
│   └── run.py
├── frontend/               # 前端应用
│   ├── src/
│   │   ├── App.tsx
│   │   ├── main.tsx
│   │   └── components/
│   │       ├── FileList.tsx
│   │       ├── Sidebar.tsx
│   │       ├── Toolbar.tsx
│   │       ├── Breadcrumb.tsx
│   │       ├── CreateFolderModal.tsx
│   │       ├── MoveModal.tsx
│   │       └── RAGPanel.tsx
│   ├── index.html
│   ├── package.json
│   └── vite.config.ts
└── README.md
```

## Roadmap

- [x] V0.1 - 基础文件浏览和管理
- [x] V0.2 - 文件索引和增量更新
- [x] V0.3 - 向量化和 RAG 问答
- [ ] V0.4 - AI 文件规划 + 审批流
- [ ] V1.0 - 完整功能发布

## License

MIT
