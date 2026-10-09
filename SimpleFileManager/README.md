# SimpleFileManager

**NASMind 的前身** - 一个轻量级的智能文件管理助手，面向树莓派 / N100 迷你主机 / NAS 部署。

## 愿景

让每个拥有本地硬件的人，都能拥有一套完整的本地 AI 文件管理方案。

- **轻量**: Docker 一键部署，资源占用可控
- **智能**: 基于本地 LLM 的文件分析、归类和检索
- **安全**: 所有数据本地处理，不上云；可选访问密码
- **可扩展**: 模块化设计，支持 OpenAI 兼容的任意模型端点

## 核心功能

### 1. 文件浏览与管理
- [x] 目录树导航 + 面包屑
- [x] 文件预览（图片、文本、代码、PDF）
- [x] 网格 / 列表 / 紧凑 三种视图
- [x] 多选（Ctrl+Click、Shift+Click 范围选）与批量操作
- [x] 移动、复制、重命名、删除、创建文件夹
- [x] 上传（多文件）/ 下载
- [x] 键盘快捷键（见下文）
- [x] 拖拽移动
- [ ] 收藏和标签

### 2. 智能索引
- [x] 自动扫描和索引文件系统
- [x] 增量更新（轮询式，可配间隔与防抖窗口）
- [x] 仅索引文本/代码类文件，跳过二进制与大文件

### 3. 向量化搜索
- [x] LanceDB 向量库 + FTS5 关键词索引
- [x] 混合检索：BM25 关键词 + 向量语义双路召回融合
- [x] 一键「AI 总结」：LLM 基于命中文件生成回答

### 4. AI 问答 (RAG)
- [x] 基于文件内容的问答
- [x] 参考文档溯源（路径 + 相似度）

### 5. Agent 执行助手
- [x] 只读工具：`list_directory` / `read_file` / `find_files` / `search_content`
- [x] 多步思考→调工具循环（步数可配）
- [x] 唯一变更途径：`submit_plan` 提交审批计划
- [x] **上下文管理**：用户可配模型上下文大小，超限自动将旧对话压缩为「工作记忆」摘要
- [x] 会话历史持久化，重启后自动恢复上下文
- [x] 审批流：待审批 → 已批准 → 执行（含执行日志与失败重试记录）

### 6. 文件整理
- [x] 目录快照（按日期）
- [x] 快照对比：新增/删除/变动统计
- [x] 按文件类型生成整理建议，可一键转为待审批计划

### 7. 知识日报
- [x] 手动生成：扫描当日变动文件，LLM 撰写 Markdown 日报
- [x] **自动生成**：两种模式
  - 定时模式：每天固定小时生成
  - 间隔模式：每 N 小时生成
- [x] LLM 不可用时降级为原始文件清单
- [x] 历史日报归档与查阅

### 8. 访问安全
- [x] 可选密码登录（PBKDF2 哈希存储）
- [x] Token 认证（7 天有效期，可退出登录）
- [x] `INIT_PASSWORD` 环境变量预置密码
- [x] CORS 来源限制

### 9. 界面
- [x] 左侧导航 Sidebar 布局
- [x] 移动端 / 平板适配（抽屉式侧栏、响应式列、自适应网格）
- [x] 亮色主题
- [x] Toast 通知 / 确认弹窗 / 右键菜单

## 快速开始

### 方式一：Docker Compose（推荐）

```bash
git clone <repo> && cd SimpleFileManager

# 1. 配置（复制 .env.example 为 .env 并按需修改）
cp .env.example .env

# 2. 启动
docker compose up -d

# 3. 访问
# http://localhost         前端 + API
# 数据持久化在 ./data/
```

### 方式二：本地开发

需要 Python 3.12+ 和 Node 20+。

**一键启动（推荐）**

```bash
./run_dev.sh --help              # 查看用法
./run_dev.sh --start all         # LLM 服务 + 后端 + 前端
./run_dev.sh --start backend     # 只启后端
./run_dev.sh --start frontend    # 只启前端
./run_dev.sh --start llm         # 只启本地推理服务
./run_dev.sh --status            # 查看各进程状态
./run_dev.sh --stop all          # 停止后端和前端（不动 LLM）
./run_dev.sh --stop llm          # 停止 LLM 服务
./run_dev.sh --restart all       # 重启
```

`--stop all` 不停 LLM 服务，因为推理进程可能被其他流程共用，需要时显式 `--stop llm`。

脚本会依次拉起 LLM 服务、FastAPI（:8000）、Vite（端口自动读取 `vite.config.ts`），
日志写到 `logs/{backend,frontend}.log`，pid 写到 `logs/*.pid`。
启动前检测端口占用，失败时打印日志末尾 20 行便于诊断。

**手动分步启动**

```bash
# LLM 服务
cd LLMServices && ./serve.sh start

# 后端
cd backend
python -m venv .venv
source .venv/bin/activate
pip install -r requirements.txt
python run.py                      # http://localhost:8000

# 前端（新开终端）
cd frontend
npm install
npm run dev                        # 端口见 vite.config.ts
```

`run.py` 支持 `BACKEND_PORT` 覆盖端口，`BACKEND_RELOAD=0` 可关闭热重载
（关闭后不 spawn 子进程，便于脚本追踪 pid）。

`.env` 放在项目根目录，开发模式通过 `backend/.env` 软链读取：

```bash
ln -s ../.env backend/.env
```

### 本地 LLM 服务（LLMServices）

推理服务跑在宿主机上，Docker 容器通过 `host.docker.internal` 访问：

```
docker backend 容器  ──►  host.docker.internal:9010  ──►  llama-server (chat)
                     └──►  host.docker.internal:9011  ──►  llama-server (embedding)
```

```bash
cd LLMServices
./serve.sh start     # Embedding :9011，Chat :9010
./serve.sh status
./serve.sh stop
```

`serve.sh` 的后端选择（每个服务可用 `LLM_BACKEND` / `EMBEDDING_BACKEND` 独立指定）：

| 后端 | 说明 |
|------|------|
| `llama` | 优先。搜索顺序：`$LLAMA_SERVER` → `./bin/llama-server` → PATH → `../references/llama.cpp/build/bin/` |
| `rmi` | 回退到 `bin/rust-model-inference` |
| `auto` | 默认。找到 llama-server 就用，否则回退 rmi |

聊天服务的上下文窗口从项目 `.env` 的 `MAX_CONTEXT_TOKENS` 自动读取，保证服务端窗口与后端压缩预算一致；临时覆盖用 `LLM_CTX=32768`。

```bash
LLM_BACKEND=rmi ./serve.sh start      # 强制用 rust 后端
LLM_CTX=131072 ./serve.sh start        # 临时改上下文
./serve.sh status                      # 显示找到的后端 + ctx + 线程数
```

> **为什么模型不进 docker-compose**：本机的 `llama-server` 是按 aarch64 板子定制的编译产物
> （实测比通用 Rust 实现快 3-4 倍），容器化需要重新复现编译过程并可能丢失加速调优。
> 应用容器化 + 推理原生跑，是当前更合理的分工。开机自启建议用 systemd 而非 compose。

相关目录 `bin/`、`models/`、`logs/` 均已被 `.gitignore` 忽略，不入库。

### 运行测试

```bash
cd backend
source .venv/bin/activate
python -m pytest tests/ -v         # 41 个测试
```

## 配置说明

所有配置项均可通过 `.env` 预置，也可在启动后于「设置」页修改（热生效，写回 `settings.json`，优先级高于 `.env`）。

| 环境变量 | 默认值 | 说明 |
|---------|--------|------|
| `INIT_PASSWORD` | 空 | 首次启动预置访问密码，留空则无密码 |
| `LLM_BASE_URL` | - | OpenAI 兼容 Chat 接口地址 |
| `LLM_MODEL` | - | 对话模型名 |
| `LLM_API_KEY` | 空 | API Key，本地推理可留空 |
| `EMBEDDING_BASE_URL` | - | Embedding 接口地址 |
| `EMBEDDING_MODEL` | - | Embedding 模型名 |
| `EMBEDDING_API_KEY` | 空 | API Key |
| `EMBEDDING_DIM` | AUTO | 向量维度，已知模型名可自动探测 |
| `INDEX_INTERVAL` | 300 | 自动扫描间隔（秒，最低 30） |
| `AUTO_INDEX_ENABLED` | true | 自动索引开关 |
| `INDEX_DEBOUNCE_SECONDS` | 10 | 防抖窗口，避免索引半成品文件 |
| `MAX_AGENT_STEPS` | 8 | Agent 单次对话最大「思考→调工具」循环数 |
| `MAX_CONTEXT_TOKENS` | 32768 | 模型上下文窗口大小（tokens），超限自动压缩历史 |
| `AUTO_DIGEST_ENABLED` | false | 自动日报开关 |
| `AUTO_DIGEST_MODE` | scheduled | `scheduled`（定时）或 `interval`（间隔） |
| `AUTO_DIGEST_HOUR` | 23 | 定时模式：每天第几小时生成 |
| `AUTO_DIGEST_INTERVAL_HOURS` | 6 | 间隔模式：每 N 小时生成 |

> `MAX_CONTEXT_TOKENS` 请按你的模型实际窗口填写（如 32768 / 131072 / 1048576）。
> 系统会预留 4096 tokens 给模型回复，超出预算时自动将最旧的一半对话压缩为摘要。

## API 接口

### 文件管理
| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/api/fs/browse` | 浏览目录 |
| GET | `/api/fs/tree` | 获取目录树 |
| POST | `/api/fs/create_folder` | 创建文件夹 |
| POST | `/api/fs/move` | 移动/重命名 |
| POST | `/api/fs/copy` | 复制 |
| POST | `/api/fs/delete` | 删除 |
| GET | `/api/fs/download` | 下载文件 |
| POST | `/api/fs/upload` | 上传文件 |
| GET | `/api/fs/content` | 读取文本内容（预览用） |

### 搜索
| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/api/search/query` | 文件名搜索 |
| POST | `/api/search/hybrid` | 混合检索（关键词 + 语义） |

### RAG / Agent / 审批
| 方法 | 路径 | 说明 |
|------|------|------|
| POST | `/api/rag/query` | RAG 问答 |
| GET | `/api/rag/status` | 索引状态 |
| POST | `/api/agent/chat` | Agent 对话 |
| GET | `/api/plans` | 计划列表 |
| POST | `/api/plans/{id}/approve` | 批准计划 |
| POST | `/api/plans/{id}/execute` | 执行计划 |

### 整理 / 日报 / 设置 / 认证
| 方法 | 路径 | 说明 |
|------|------|------|
| POST | `/api/organizer/snapshot` | 拍摄快照 |
| GET | `/api/organizer/compare` | 对比快照 |
| POST | `/api/digest/generate` | 生成日报 |
| GET | `/api/digest/list` | 日报列表 |
| GET/POST | `/api/settings` | 读取/更新设置 |
| POST | `/api/auth/login` | 登录获取 token |
| POST | `/api/auth/password` | 设置/修改密码 |

除 `/api/health` 和 `/api/auth/*` 外，所有接口在设置密码后都需要 `Authorization: Bearer <token>`。

## 键盘快捷键

| 按键 | 功能 |
|------|------|
| `Ctrl + A` | 全选当前目录文件 |
| `Delete` | 删除选中项 |
| `F2` | 重命名选中项 |
| `F5` | 刷新当前目录 |
| `Ctrl + C` | 复制选中项 |
| `Ctrl + V` | 粘贴到当前目录 |
| `Esc` | 取消选择 / 关闭弹窗 |

## 文件位置速查

### 配置

| 文件 | 用途 | 是否入库 |
|------|------|---------|
| `.env` | 实际配置（Docker 自动读；开发模式经 `backend/.env` 软链读取） | ❌ gitignore |
| `.env.example` | 配置模板，含全部可用变量与默认值说明 | ✅ |
| `backend/.env` | 指向 `../.env` 的软链，让 `python-dotenv` 在 `backend/` 下工作 | ❌ |
| `data/.simplefilemanager/settings.json` | 设置页保存的运行时配置，优先级高于 `.env` | ❌ |

### 程序数据（全部在 `data/.simplefilemanager/`，gitignore）

| 文件 | 内容 | 删掉的后果 |
|------|------|-----------|
| `vector_db/` | LanceDB 向量库 | 语义检索失效，需重新索引 |
| `search.db` | SQLite FTS5 关键词索引（trigram 分词，支持中文子串） | 关键词检索退化，语义检索仍可用 |
| `file_index.db` | 已索引文件清单（mtime/size） | 下次扫描会全量重新索引 |
| `chat_history.db` | 会话与消息记录 | 历史对话丢失 |
| `plans.db` | 审批计划 + 执行日志 | 计划记录丢失，文件不受影响 |
| `organizer.db` | 目录快照 | 历史快照丢失 |
| `digest.db` | 已生成的知识日报 | 日报丢失 |
| `auth.json` | 密码哈希 + 有效 token | 重启后按 `INIT_PASSWORD` 重新初始化 |
| `index_stats.json` | 索引统计缓存 | 重新统计 |

### 模型与推理服务

| 路径 | 内容 | 是否入库 |
|------|------|---------|
| `LLMServices/models/llm/` | LLM GGUF 模型（约 8.4 GB） | ❌ |
| `LLMServices/models/embedding/` | Embedding GGUF 模型（约 610 MB） | ❌ |
| `LLMServices/bin/` | 推理二进制（`llama-server` / `rust-model-inference`） | ❌ |
| `LLMServices/logs/` | 服务日志与 pid 文件 | ❌ |
| `LLMServices/serve.sh` | 服务管理脚本（start/stop/status/restart） | ✅ |
| `run_dev.sh` | 开发栈管理器（`--start/--stop/--status/--restart`） | ✅ |
| `logs/` | 开发模式日志与 pid 文件（`backend.log` / `frontend.log` / `*.pid`） | ❌ |
| `~/Repos/rust-model-inference/references/llama.cpp/build/bin/llama-server` | 自编译的 llama-server，`serve.sh` 会自动搜索到 | 仓库外 |

### 备份建议

迁移机器或重装时，只需带走：

1. `data/` 整个目录（含用户文件和 `.simplefilemanager/`）
2. `.env`（如果不想重新填 API Key）

`LLMServices/models/` 可以不带走——模型文件通常单独存放或从源重新下载。

## 项目结构

```
SimpleFileManager/
├── docker-compose.yml        # Docker 编排
├── run_dev.sh                # 开发栈管理（start/stop/status/restart）
├── .env.example              # 配置模板
├── data/                     # 用户文件 + 程序数据（gitignore）
│   └── .simplefilemanager/
│       ├── vector_db/        # LanceDB 向量库
│       ├── search.db         # FTS5 关键词索引
│       ├── file_index.db     # 索引清单
│       ├── chat_history.db   # 会话历史
│       ├── plans.db          # 审批计划与执行日志
│       ├── organizer.db      # 目录快照
│       ├── digest.db         # 日报
│       ├── auth.json         # 密码哈希与 token
│       └── settings.json     # 应用设置
├── LLMServices/              # 本地推理服务（bin/logs/models 不入库）
│   ├── serve.sh              # start/stop/status/restart
│   ├── bin/                  # 推理二进制（gitignore）
│   ├── models/
│   │   ├── llm/              # LFM2.5-8B-A1B-Q8_0.gguf ~8.4GB（gitignore）
│   │   └── embedding/        # Qwen3-Embedding-0.6B-Q8_0.gguf ~610MB（gitignore）
│   └── logs/                 # 运行日志 + pid（gitignore）
├── backend/
│   ├── app/
│   │   ├── main.py           # FastAPI 入口 + lifespan
│   │   ├── deps.py           # 状态、向量库、RAG、SQLite 服务、路径安全
│   │   ├── llm_client.py     # openai SDK 封装（含批量 embedding）
│   │   ├── indexer.py        # 增量索引器
│   │   ├── digest.py         # 日报生成
│   │   ├── auto_digest.py    # 自动日报调度
│   │   ├── search_engine.py  # FTS5 + 混合检索
│   │   ├── models.py         # Pydantic 模型
│   │   └── routers/          # fs/search/rag/chat/agent/plans/organizer/digest/settings/auth
│   └── tests/                # 41 个 pytest 测试
└── frontend/
    └── src/
        ├── auth.ts           # token 管理与 authFetch
        ├── api.ts            # API 封装
        ├── components/
        │   ├── FileManagerPage.tsx   # 主界面（含快捷键）
        │   ├── Sidebar.tsx           # 文件树
        │   ├── FileExplorer/         # 网格/列表/紧凑视图、工具栏、面包屑
        │   ├── FilePreview.tsx       # 文件预览
        │   ├── SearchPage.tsx        # 混合检索 + AI 总结
        │   ├── IndexPage.tsx         # 索引管理
        │   ├── OrganizerPage.tsx     # 整理 + 审批中心
        │   ├── DigestPage.tsx        # 知识日报
        │   ├── SimpleChat.tsx        # Agent 对话
        │   ├── SettingsPage.tsx      # 设置
        │   ├── LoginPage.tsx         # 登录
        │   └── ui/                   # Icon/Toast/Dialog/ContextMenu/ChatLayout
        └── hooks/
```

## Roadmap

- [x] V0.1 - 基础文件浏览和管理
- [x] V0.2 - 文件索引和增量更新
- [x] V0.3 - 向量化和 RAG 问答
- [x] V0.4 - AI 文件规划 + 审批流
- [x] V0.4 - Agent 上下文压缩与会话恢复
- [x] V0.4 - 登录认证
- [x] V0.4 - Docker 部署
- [x] V0.4 - 移动端适配
- [x] V0.4 - 自动日报（定时/间隔）
- [ ] V0.5 - 文件系统事件监听（替代轮询）
- [ ] V0.5 - 收藏和标签
- [ ] V1.0 - 完整功能发布

## License

MIT
