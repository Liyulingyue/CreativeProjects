"""
MiniClaw Backend - Entry Point
"""
import uvicorn
from app.main import app

if __name__ == "__main__":
    uvicorn.run(
        app,
        host="0.0.0.0",
        port=8000,
        log_level="info",
        reload=False  # 生产环境应设为False
    )