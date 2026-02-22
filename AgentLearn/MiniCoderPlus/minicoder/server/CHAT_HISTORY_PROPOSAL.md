# Chat History Storage Proposal

## 目的
在后端持久化存储聊天记录，以支持：会话恢复、历史回溯、搜索、导出与审计。后端存储能保持单一来源的真相，便于管理与备份。

## 总体设计
- 使用轻量关系型数据库（SQLite）作为默认实现，生产环境可切换到 PostgreSQL。
- 在 `minicoder/core` 下添加一个 `db.py` 模块，采用 SQLAlchemy ORM（或轻量 sqlite3 wrapper）。
- 通过 `session_id` 与 `workspace` 作为主索引进行分组。

## 数据模型（示例 SQL）
```sql
CREATE TABLE IF NOT EXISTS chat_messages (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  session_id TEXT NOT NULL,
  workspace TEXT,
  role TEXT NOT NULL,        -- user | assistant | system
  content TEXT NOT NULL,
  metadata TEXT,             -- JSON string, 可选（tool_calls 等）
  timestamp DATETIME DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_chat_session ON chat_messages(session_id);
CREATE INDEX IF NOT EXISTS idx_chat_workspace ON chat_messages(workspace);
```

## 后端接口建议（新增/扩展）
- `GET /api/v1/chat/history?session_id={id}&workspace={ws}&limit=100`  
  返回按时间排序的消息列表。

- `DELETE /api/v1/chat/history?session_id={id}&workspace={ws}`  
  删除指定会话/工作区的历史（需谨慎，可增加确认或软删除）。

- `POST /api/v1/chat` 在处理请求后，将新的 user/assistant 消息写入数据库（非必须，但建议在非流式与流式完成后保存最终历史）。

实现要点：在 `minicoder/server/app.py` 的 `/chat` 处理逻辑处，在 agent 生成消息时把消息存入 DB（流式模式可在最终 `done` 阶段回写完整历史或逐条写入）。

## 模块与位置
- 新增：`minicoder/core/db.py`（数据库连接、模型、辅助函数）
- 修改：`minicoder/server/app.py`（在 chat 接口中调用 DB 保存与查询）

## 事务与并发
- SQLite 在多线程写入时需注意：使用 SQLAlchemy 的 `check_same_thread=False` 与合适的会话管理，或在高并发下推荐 PostgreSQL。
- 写入应使用短事务（单条消息入库或批量最终写入），避免长事务阻塞。

## 隐私与安全
- 聊天记录可能包含敏感数据：应在生产中考虑加密-at-rest、访问控制和日志清理策略。
- 对 `DELETE` 与导出操作添加鉴权（当前 demo 环境允许所有来源）。

## 迁移与兼容性
- 初始实现使用 SQLite 数据库文件（例如 `data/chat_history.db`），无需外部依赖。生产可通过环境变量指向 PostgreSQL URL。
- 保持 API 向后兼容：若前端未使用历史接口，现有行为不受影响。

## 前端修改（概要）
- 在 `Workbench` 页面（`frontend/web/src/routes/Workbench.tsx`）如果存在 `sessionId`，在组件挂载时调用：
  `GET /api/v1/chat/history?session_id={sessionId}&workspace={workspace}` 并将返回的消息注入消息列表。
- 添加“清除历史”或“加载历史”按钮，调用相应的 API。

## 示例：快速 curl 调用
- 获取历史：
```bash
curl "http://localhost:8000/api/v1/chat/history?session_id=session_abc&limit=200"
```
- 删除历史：
```bash
curl -X DELETE "http://localhost:8000/api/v1/chat/history?session_id=session_abc"
```

## 迁移步骤（简短）
1. 添加 `minicoder/core/db.py` 与依赖（SQLAlchemy）。
2. 在 `minicoder/server/app.py` 中引入 DB 并在 `/chat` 中保存消息；添加 `/chat/history` 与 `DELETE` 路由。
3. 在开发环境测试读写、并发写入行为。4. 更新 `requirements.txt`（可选）。

## 风险与注意事项
- SQLite 并发写入限制：高并发场景建议 PostgreSQL。
- 聊天记录增长：实现自动清理或存档策略来控制磁盘占用。

---

如果您确认，我可以：
- （1）实现 `minicoder/core/db.py` 与迁移 SQL；
- （2）在 `minicoder/server/app.py` 中新增历史查询与删除路由；
- （3）在 `frontend` 中添加基本加载历史按钮（在 `Workbench`）。

请告诉我您想先从哪一步开始。