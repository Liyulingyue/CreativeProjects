from fastapi import FastAPI
from fastapi.middleware.cors import CORSMiddleware
from .api.endpoints import router as api_router
from .database import init_db
from .core.config import USE_DATABASE

app = FastAPI(title="FileStation API", version="0.3.0")

# Add CORS middleware
app.add_middleware(
    CORSMiddleware,
    allow_origins=["*"],
    allow_credentials=True,
    allow_methods=["*"],
    allow_headers=["*"],
)

# Initialize database on startup if enabled
if USE_DATABASE:
    init_db()

# Include dynamic routes
app.include_router(api_router)

@app.get("/")
def root():
    return {"message": "FileStation API", "status": "running"}
