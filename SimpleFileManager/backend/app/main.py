from fastapi import FastAPI
from fastapi.middleware.cors import CORSMiddleware
from .routers import fs, search, settings, rag, chat, chat_history_router, agent, organizer

app = FastAPI(title="SimpleFileManager", version="0.2.0")

app.add_middleware(
    CORSMiddleware,
    allow_origins=["*"],
    allow_credentials=True,
    allow_methods=["*"],
    allow_headers=["*"],
)

app.include_router(fs, prefix="/api/fs", tags=["fs"])
app.include_router(search, prefix="/api/search", tags=["search"])
app.include_router(settings, prefix="/api/settings", tags=["settings"])
app.include_router(rag, prefix="/api/rag", tags=["rag"])
app.include_router(chat, prefix="/api/chat", tags=["chat"])
app.include_router(chat_history_router, prefix="/api/chat_history", tags=["chat_history"])
app.include_router(agent, prefix="/api/agent", tags=["agent"])
app.include_router(organizer, prefix="/api/organizer", tags=["organizer"])


@app.get("/api/health")
def health():
    return {"status": "ok", "app": "SimpleFileManager", "version": "0.1.0"}
