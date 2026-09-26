from contextlib import asynccontextmanager
import os
from fastapi import FastAPI, Depends
from fastapi.middleware.cors import CORSMiddleware
from .routers import fs, search, settings, rag, chat, chat_history_router, agent, organizer, plans, digest, auth, get_current_auth
from .routers.auth import is_auth_enabled, set_password
from .indexer import indexer


def _init_auth():
    """If INIT_PASSWORD env var is set and no password exists yet, set it on startup."""
    init_pwd = os.getenv("INIT_PASSWORD", "").strip()
    if init_pwd and not is_auth_enabled():
        set_password(init_pwd)


@asynccontextmanager
async def lifespan(app: FastAPI):
    _init_auth()
    indexer.start()
    yield
    indexer.stop()


app = FastAPI(title="SimpleFileManager", version="0.4.0", lifespan=lifespan)

app.add_middleware(
    CORSMiddleware,
    allow_origins=["*"],
    allow_credentials=False,
    allow_methods=["*"],
    allow_headers=["*"],
)

# Auth router is public (no dependency)
app.include_router(auth, prefix="/api/auth", tags=["auth"])

# Health check is public
@app.get("/api/health")
def health():
    return {"status": "ok", "app": "SimpleFileManager", "version": "0.4.0"}

# All other routers require auth
auth_dep = Depends(get_current_auth)
app.include_router(fs, prefix="/api/fs", tags=["fs"], dependencies=[auth_dep])
app.include_router(search, prefix="/api/search", tags=["search"], dependencies=[auth_dep])
app.include_router(settings, prefix="/api/settings", tags=["settings"], dependencies=[auth_dep])
app.include_router(rag, prefix="/api/rag", tags=["rag"], dependencies=[auth_dep])
app.include_router(chat, prefix="/api/chat", tags=["chat"], dependencies=[auth_dep])
app.include_router(chat_history_router, prefix="/api/chat_history", tags=["chat_history"], dependencies=[auth_dep])
app.include_router(agent, prefix="/api/agent", tags=["agent"], dependencies=[auth_dep])
app.include_router(organizer, prefix="/api/organizer", tags=["organizer"], dependencies=[auth_dep])
app.include_router(plans, tags=["plans"], dependencies=[auth_dep])
app.include_router(digest, tags=["digest"], dependencies=[auth_dep])
