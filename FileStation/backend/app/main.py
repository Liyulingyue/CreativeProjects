from fastapi import FastAPI
from fastapi.middleware.cors import CORSMiddleware
from .api.endpoints import router as api_router
from .database import init_db
from .core.config import settings

app = FastAPI(title=settings.APP_NAME, version=settings.VERSION)

# Add CORS middleware
app.add_middleware(
    CORSMiddleware,
    allow_origins=["*"],
    allow_credentials=True,
    allow_methods=["*"],
    allow_headers=["*"],
)

# Initialize database on startup if enabled
if settings.USE_DATABASE:
    init_db()

# Include dynamic routes
app.include_router(api_router)

@app.get("/")
def root():
    return {"message": "FileStation API", "status": "running"}
