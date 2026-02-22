# Server 模块化重构计划

## 目标
将单一 `app.py` 分解为：路由层、服务层、数据模型层，便于扩展与维护。

## 新架构

```
minicoder/server/
├── __init__.py
├── app.py                         [核心初始化，仅 50 行]
├── models.py                      [所有 Pydantic 请求/响应模型]
├── routers/
│   ├── __init__.py
│   ├── chat.py                   [POST /chat, GET /chat/history, DELETE /chat/history]
│   ├── files.py                  [GET /files/list, GET /files/read]
│   ├── workspace.py              [GET /workspace/resolve]
│   └── terminal.py               [WebSocket /ws/terminal/{session_id}]
└── services/
    ├── __init__.py
    ├── chat_service.py           [Agent 调用 + 聊天历史 DB 操作]
    └── file_service.py           [文件读写逻辑]
```

## 职责划分

| 层 | 文件 | 职责 |
|----|----|------|
| **初始化** | `app.py` | 创建 FastAPI app，注册 CORS，include 所有 routers |
| **数据模型** | `models.py` | Pydantic 请求/响应类（ChatRequest, ChatResponse, etc.） |
| **路由** | `routers/*.py` | HTTP 请求接收、参数验证、调用 service、返回响应 |
| **业务逻辑** | `services/*.py` | Agent 执行、数据库操作、文件系统操作 |

## 详细文件设计

### `app.py` (简化后)
```python
from fastapi import FastAPI
from fastapi.middleware.cors import CORSMiddleware
from .routers import chat, files, workspace, terminal

app = FastAPI(title="MiniCoder Plus API")

# CORS
app.add_middleware(CORSMiddleware, allow_origins=["*"], ...)

# 注册所有 routers
app.include_router(chat.router, prefix="/api/v1")
app.include_router(files.router, prefix="/api/v1")
app.include_router(workspace.router, prefix="/api/v1")
app.include_router(terminal.router)

@app.get("/")
async def root():
    return {"status": "ok", "message": "MiniCoder Plus API is running"}

def run_server(host="0.0.0.0", port=8000):
    import uvicorn
    uvicorn.run(app, host=host, port=port)
```

### `models.py`
```python
from pydantic import BaseModel
from typing import List, Optional, Dict

class ChatRequest(BaseModel):
    prompt: str
    history: Optional[List[Dict]] = None
    session_id: Optional[str] = None
    workspace: Optional[str] = None

class ChatResponse(BaseModel):
    response: str
    history: List[Dict]

class ChatHistoryItem(BaseModel):
    id: int
    session_id: str
    workspace: Optional[str]
    role: str
    content: str
    timestamp: str

# ... 其他模型
```

### `routers/chat.py`
```python
from fastapi import APIRouter, HTTPException
from ...services.chat_service import ChatService
from ..models import ChatRequest, ChatResponse, ChatHistoryItem

router = APIRouter(tags=["chat"])
service = ChatService()

@router.post("/chat", response_model=ChatResponse)
async def chat(request: ChatRequest, stream: bool = False):
    # 接收请求 → 调用 service → 返回响应
    return await service.handle_chat(request, stream)

@router.get("/chat/history", response_model=List[ChatHistoryItem])
async def get_history(session_id: str, workspace: Optional[str] = None, limit: int = 100):
    # 查询历史
    return service.get_history(session_id, workspace, limit)

@router.delete("/chat/history")
async def delete_history(session_id: str, workspace: Optional[str] = None):
    # 删除历史
    service.delete_history(session_id, workspace)
    return {"message": "deleted"}
```

### `routers/files.py`
```python
from fastapi import APIRouter, HTTPException
from ...services.file_service import FileService

router = APIRouter(tags=["files"])
service = FileService()

@router.get("/files/list")
async def list_files(path: Optional[str] = None):
    return await service.list_files(path)

@router.get("/files/read")
async def read_file(path: str):
    return await service.read_file(path)
```

### `services/chat_service.py`
```python
from ...core.settings import settings
from ...agents.agent import MiniCoderAgent
from ...core.terminal_manager import terminal_manager
from ...tools import CodeTools, set_current_workspace
from ...core.db import ChatDatabase  # 新增，见下文
from ..models import ChatRequest, ChatResponse
import asyncio
import json

class ChatService:
    def __init__(self):
        self.agent = MiniCoderAgent()
        self.db = ChatDatabase()
    
    async def handle_chat(self, request: ChatRequest, stream: bool = False):
        # 原有逻辑，但改为调用 db.save_message() 保存
        # 使用 self.db 存储消息
        ...
    
    def get_history(self, session_id: str, workspace: Optional[str] = None, limit: int = 100):
        # 从 DB 查询历史
        return self.db.get_messages(session_id, workspace, limit)
    
    def delete_history(self, session_id: str, workspace: Optional[str] = None):
        # 从 DB 删除历史
        self.db.delete_messages(session_id, workspace)
```

## 新增依赖

在 `minicoder/core/db.py`：
```python
import sqlite3
from pathlib import Path
from typing import List, Dict, Optional
from datetime import datetime

class ChatDatabase:
    def __init__(self, db_path: str = "data/chat_history.db"):
        self.db_path = Path(db_path)
        self.db_path.parent.mkdir(exist_ok=True)
        self._init_db()
    
    def _init_db(self):
        """创建表"""
        conn = sqlite3.connect(self.db_path)
        cursor = conn.cursor()
        cursor.execute("""
            CREATE TABLE IF NOT EXISTS chat_messages (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                session_id TEXT NOT NULL,
                workspace TEXT,
                role TEXT NOT NULL,
                content TEXT NOT NULL,
                metadata TEXT,
                timestamp DATETIME DEFAULT CURRENT_TIMESTAMP
            )
        """)
        cursor.execute("""
            CREATE INDEX IF NOT EXISTS idx_session 
            ON chat_messages(session_id, workspace)
        """)
        conn.commit()
        conn.close()
    
    def save_message(self, session_id: str, role: str, content: str, workspace: Optional[str] = None):
        """保存单条消息"""
        conn = sqlite3.connect(self.db_path)
        cursor = conn.cursor()
        cursor.execute("""
            INSERT INTO chat_messages (session_id, role, content, workspace)
            VALUES (?, ?, ?, ?)
        """, (session_id, role, content, workspace))
        conn.commit()
        conn.close()
    
    def get_messages(self, session_id: str, workspace: Optional[str] = None, limit: int = 100) -> List[Dict]:
        """查询历史"""
        conn = sqlite3.connect(self.db_path)
        cursor = conn.cursor()
        if workspace:
            cursor.execute("""
                SELECT id, session_id, workspace, role, content, timestamp
                FROM chat_messages
                WHERE session_id = ? AND workspace = ?
                ORDER BY timestamp DESC
                LIMIT ?
            """, (session_id, workspace, limit))
        else:
            cursor.execute("""
                SELECT id, session_id, workspace, role, content, timestamp
                FROM chat_messages
                WHERE session_id = ?
                ORDER BY timestamp DESC
                LIMIT ?
            """, (session_id, limit))
        
        rows = cursor.fetchall()
        conn.close()
        
        return [
            {
                "id": row[0],
                "session_id": row[1],
                "workspace": row[2],
                "role": row[3],
                "content": row[4],
                "timestamp": row[5]
            }
            for row in rows
        ]
    
    def delete_messages(self, session_id: str, workspace: Optional[str] = None):
        """删除历史"""
        conn = sqlite3.connect(self.db_path)
        cursor = conn.cursor()
        if workspace:
            cursor.execute("""
                DELETE FROM chat_messages
                WHERE session_id = ? AND workspace = ?
            """, (session_id, workspace))
        else:
            cursor.execute("""
                DELETE FROM chat_messages
                WHERE session_id = ?
            """, (session_id,))
        conn.commit()
        conn.close()
```

## 迁移步骤

1. 创建新目录和文件结构
2. 创建 `models.py`，移出所有模型
3. 创建 `routers/` 子目录和各路由文件
4. 创建 `services/` 子目录和业务逻辑
5. 创建 `minicoder/core/db.py`（数据库层）
6. 简化 `app.py`，仅保留初始化和 include
7. 运行测试验证

## 优势

✅ 清晰的关注点分离（SoC）  
✅ 便于添加新功能（新路由 → 新 service）  
✅ 易于单元测试（每层可独立测试）  
✅ 代码复用性好（同一 service 可被多个路由调用）  
✅ Git 历史清晰（改动分散到多个文件，逻辑变更突出）
