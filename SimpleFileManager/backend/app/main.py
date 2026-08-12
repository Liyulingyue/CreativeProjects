from fastapi import FastAPI
from fastapi.middleware.cors import CORSMiddleware
from .routers import fs, search, settings, rag, chat

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


@app.get("/api/health")
def health():
    return {"status": "ok", "app": "SimpleFileManager", "version": "0.1.0"}
