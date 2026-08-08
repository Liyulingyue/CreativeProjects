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
- 目录树导航
- 文件预览（图片、文本、PDF 等）
- 批量操作（移动、复制、删除）
- 收藏和标签

### 2. 智能索引
- 自动扫描和索引文件系统
- 增量更新，监控变动
- 支持多种文件类型识别

### 3. 向量化搜索
- 文件内容向量化
- 自然语言搜索
- 语义相似度匹配

### 4. AI 文件规划 (规划中)
- 自动分析文件结构和内容
- 提出归档建议
- 审批机制确保安全

### 5. Agent 执行助手 (规划中)
- 复杂文件操作自动化
- React Tools 调度

## 技术架构

```
┌─────────────────────────────────────────────────────────────┐
│                     SimpleFileManager                        │
├───────────────────────────┬─────────────────────────────────┤
│        Frontend           │           Backend               │
│   (vite + React + TS)     │        (FastAPI / Rust)         │
├───────────────────────────┴─────────────────────────────────┤
│                      Service Layer                           │
│   ┌─────────────┐  ┌─────────────┐  ┌─────────────────────┐  │
│   │ FileService │  │ IndexService│  │  PlanningService   │  │
│   └─────────────┘  └─────────────┘  └─────────────────────┘  │
├─────────────────────────────────────────────────────────────┤
│                      Storage Layer                            │
│   ┌─────────────┐  ┌─────────────┐  ┌─────────────────────┐  │
│   │  SQLite     │  │  LanceDB    │  │   File System      │  │
│   │ (metadata)  │  │ (vectors)   │  │                    │  │
│   └─────────────┘  └─────────────┘  └─────────────────────┘  │
├─────────────────────────────────────────────────────────────┤
│                      Model Layer                              │
│   ┌─────────────────────────────────────────────────────────┐│
│   │              OpenAI Compatible API                     ││
│   │         (Ollama / vLLM / llama.cpp)                     ││
│   └─────────────────────────────────────────────────────────┘│
└─────────────────────────────────────────────────────────────┘
```

## 快速开始

### 后端

```bash
cd backend
python -m venv .venv
source .venv/bin/activate  # Linux/Mac
# .\.venv\\Scripts\\Activate.ps1  # Windows
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

创建 `backend/.env` 文件：

```env
OPENAI_API_KEY=your-api-key-here
OPENAI_BASE_URL=https://api.minimaxi.com/v1
EMBEDDING_MODEL=text-embedding-3-small
INDEX_INTERVAL=300  # 索引扫描间隔（秒）
STORAGE_PATH=/data  # 文件存储根路径
```

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
│   │       └── settings.py # 设置路由
│   ├── requirements.txt
│   └── run.py
├── frontend/               # 前端应用
│   ├── src/
│   │   ├── App.tsx
│   │   ├── main.tsx
│   │   └── components/
│   ├── index.html
│   ├── package.json
│   └── vite.config.ts
└── README.md
```

## Roadmap

- [x] V0.1 - 基础文件浏览和管理
- [x] V0.2 - 文件索引和增量更新
- [ ] V0.3 - 向量化和语义搜索
- [ ] V0.4 - AI 文件规划 + 审批流
- [ ] V1.0 - 完整功能发布

## License

MIT
