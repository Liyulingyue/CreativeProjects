from fastapi import FastAPI
from fastapi.middleware.cors import CORSMiddleware
from .routers import fs, search, settings

app = FastAPI(title="SimpleFileManager", version="0.1.0")

app.add_middleware(
    CORSMiddleware,
    allow_origins=["*"],
    allow_credentials=True,
    allow_methods=["*"],
    allow_headers=["*"],
)

app.include_router(fs.router, prefix="/api/fs", tags=["fs"])
app.include_router(search.router, prefix="/api/search", tags=["search"])
app.include_router(settings.router, prefix="/api/settings", tags=["settings"])


@app.get("/api/health")
def health():
    return {"status": "ok", "app": "SimpleFileManager", "version": "0.1.0"}
