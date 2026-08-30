from fastapi import FastAPI
from fastapi.middleware.cors import CORSMiddleware
from .routers import fs, search, settings, rag, chat, chat_history_router, agent, organizer, plans, digest
from .indexer import indexer

app = FastAPI(title="SimpleFileManager", version="0.4.0")

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
app.include_router(plans, tags=["plans"])
app.include_router(digest, tags=["digest"])


@app.on_event("startup")
def startup():
    indexer.start()


@app.on_event("shutdown")
def shutdown():
    indexer.stop()


@app.get("/api/health")
def health():
    return {"status": "ok", "app": "SimpleFileManager", "version": "0.4.0"}
