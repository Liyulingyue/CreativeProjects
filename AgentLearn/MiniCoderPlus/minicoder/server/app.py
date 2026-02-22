#!/usr/bin/env python
"""minicoder/server/app.py — FastAPI application factory and initialization."""
from fastapi import FastAPI
from fastapi.middleware.cors import CORSMiddleware
from .routers import chat, files, workspace, terminal


def create_app() -> FastAPI:
    """Create and configure the FastAPI application."""
    app = FastAPI(
        title="MiniCoder Plus API",
        description="Unified agent assistant with chat, code execution, and file management",
        version="1.0.0"
    )
    
    # Add CORS Middleware
    app.add_middleware(
        CORSMiddleware,
        allow_origins=["*"],  # For development. Change for production.
        allow_credentials=True,
        allow_methods=["*"],
        allow_headers=["*"],
    )
    
    # Root endpoint
    @app.get("/")
    async def root():
        return {
            "status": "ok",
            "message": "MiniCoder Plus API is running",
            "version": "1.0.0"
        }
    
    # Include routers
    app.include_router(chat.router, prefix="/api/v1")
    app.include_router(files.router, prefix="/api/v1")
    app.include_router(workspace.router, prefix="/api/v1")
    app.include_router(terminal.router)
    
    return app


# Create app instance
app = create_app()


def run_server(host: str = "0.0.0.0", port: int = 8000):
    """Run the FastAPI server using Uvicorn."""
    import uvicorn
    uvicorn.run(app, host=host, port=port)
