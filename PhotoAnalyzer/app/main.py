from fastapi import FastAPI
from fastapi.middleware.cors import CORSMiddleware
from .routers import dirs, files, analysis, dedup, settings, fs

app = FastAPI(title="PhotoAnalyzer", version="1.0.0")

app.add_middleware(
    CORSMiddleware,
    allow_origins=["*"],
    allow_credentials=True,
    allow_methods=["*"],
    allow_headers=["*"],
)

app.include_router(dirs.router)
app.include_router(files.router)
app.include_router(analysis.router)
app.include_router(dedup.router)
app.include_router(settings.router)
app.include_router(fs.router)


@app.get("/api/health")
def health():
    return {"status": "ok"}
