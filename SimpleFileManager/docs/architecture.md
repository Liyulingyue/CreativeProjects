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

#### 搬动、复制、改名：三条规矩

**一、不覆盖。** 目标位置已经有东西时，复制会问「已经存在」，并把原来
那份的信息带上；移动直接拒绝。浏览器里的复制本来就准备了改名重试，
因为后端从不报错，那段重试从来没触发过——现在它会触发了。

**二、搬走的东西，索引跟着搬。** 文字索引记的是绝对路径，图片索引记
的是用户看见的相对路径，两边都得跟上（整理计划自动搬文件时也一样）。
不跟上的话，搜索会一直给出一个已经不在那儿的东西。

**三、删掉的东西进回收站，不是消失。** 永久删除才真的删掉——连它的
旧版本一起（恢复不了的东西，占着磁盘没用）。

回收站也认家里的规矩：一个人被关在外的文件夹，里面出来的文件他不
该看见，还原也要看原来那个地方自己能不能写。

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

#### 同时发生的事

两个人同时用一个共享盘，谁也说不准谁先谁后。所以有几条规矩：

- **临时文件的名字必须是唯一的。** 凡是「先写进临时文件，再搬到目的地」
  的地方，临时文件的名字里都带一段随机数。名字是按目的地算出来的话，
  两件同时发生的事就会用同一个临时文件：先做完的那个把它搬走了，另一个
  就找不到自己的文件（`FileNotFoundError`）。这是真的发生过的事：六个人
  同时改设置，五个是 500。
- **搬进目的地时不能顶掉别人的文件。** 同名上传用的是硬链接而不是改名：
  链接在名字被占了的时候会失败，改名会悄悄把别人的文件顶掉。所以同时
  上传同一个文件名，结果是「一个成功，其余三个被告知名字被占了」，
  而不是「一个成功，其余的不知道去哪了」。
- **写锁只挡写。** 数据库开了 WAL，读不拿锁；写用 `db.write_lock`，
  最多等 10 秒，等不到就说得清楚（`TimeoutError` → 503），不会无声地
  把请求拖死。
- **并发是测过的。** `backend/tests/test_concurrency.py` 同时上传、浏览、
  列目录、改设置，看会不会死锁，以及结束后各个存储是不是还能用。

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

## 各处的上限

机器是有限的，硬盘是有限的，模型也是有限的。下面这些数字决定服务在
什么情况下会说「不」。

| 地方 | 上限 | 到了会怎样 |
|---|---|---|
| 上传的一块 | 16 MB（`MAX_CHUNK_BYTES`） | 413，当场停，不读进内存 |
| 技能包解压后 | 50 MB / 200 个文件 | 拒绝导入 |
| Word/Excel/PPT 解压后 | 200 MB / 2 万个文件 | 拒绝解析 |
| 文档提取出来的字 | 40 万字 | 截断 |
| 沙箱里的进程内存 | 4 GB（RLIMIT_AS） | 分配失败 |
| 版本历史（单个文件） | 512 MB | 不留版本 |
| 版本历史（全部） | 1 GB | 从最旧的开始扔 |
| 按名字搜索看过的文件 | 5 万个 | 停下，页面上说「可能还有没搜到的」 |
| 对话的上下文 | 模型自己说的容量（/props），和设置里较小的那个 | 压缩历史 |
| 沙箱命令 | 120 秒 | 超时杀掉 |

## 服务挂了会怎样

机器是有限的：30 GB 内存，24B 模型一个就占 18 GB。有一天下午内存吃紧，
内核的 OOM killer 一次性把后端、模型、图片索引服务全带走了，而**没有
人看着**，得靠人自己发现。这里记下现在的做法。

### 两个健康检查

| 接口 | 问什么 | 挂了会怎样 |
|---|---|---|
| `/api/health` | 数据库、磁盘、向量库、模型端点、内存，都还好吗 | 503（`error`）或 `degraded` |
| `/api/health/live` | 这个进程还在回话吗 | 200，永远 |

`/api/health` 在模型没起来时会说 `degraded` 并回 503——这是实话。
但 **docker-compose 里的 frontend 等的是 backend 的 `service_healthy`**，
用完整检查的话，模型重启一次，整个应用就再也起不来了。所以容器探针
用 `/api/health/live`：它只问「进程还在不在」。

内存也进了健康检查：可用内存低于 `HEALTH_MEMORY_LOW_MB`（默认 2048 MB）
就算 `degraded`，页面上会显示「内存不多了（可用 X GB）」。内核开始杀进程
之前，先说一声。

### 看门狗

`run_dev.sh` 起的看门狗每 30 秒看一次：

| 看什么 | 怎么判断 | 怎么办 |
|---|---|---|
| 后端 | `GET /api/health/live` | 重新 `run_dev.sh --start backend` |
| 24B 模型 | `GET :9010/health` | `LLMServices/serve.sh start`（两分钟一次，不然模型来不及起来） |

`./run_dev.sh --stop all` 会把看门狗一起停掉——不然几秒钟后它又会把
服务全拉起来。

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
