#!/usr/bin/env python
"""minicoder/core/settings.py — handles environment variables and common config."""
import os
from pathlib import Path

# Try to load .env from the project root (up 2 levels from here)
try:
    from dotenv import load_dotenv
    load_dotenv(Path(__file__).parent.parent.parent / ".env")
    print(f"\033[90m[Config] Loaded .env from {Path(__file__).parent.parent.parent / '.env'}\033[0m")
except ImportError:
    print("python-dotenv not found, skipping .env loading. Make sure to set environment variables manually.")

class Settings:
    """Central configuration for MiniCoder Plus."""
    
    # LLM Settings
    MODEL_KEY = os.getenv("MODEL_KEY", "")
    MODEL_URL = os.getenv("MODEL_URL", "https://api.openai.com/v1")
    MODEL_NAME = os.getenv("MODEL_NAME", "gpt-4o")
    
    # Server Settings
    SERVER_HOST = os.getenv("SERVER_HOST", "0.0.0.0")
    SERVER_PORT = int(os.getenv("SERVER_PORT", 8000))
    
    # Paths
    BASE_DIR = Path(__file__).parent.parent.parent
    WORKSPACE_DIR = BASE_DIR / "WorkSpace"
    
settings = Settings()

# Ensure WorkSpace exists
settings.WORKSPACE_DIR.mkdir(parents=True, exist_ok=True)

# Print initialization info (once per session)
if settings.MODEL_KEY:
    masked_key = f"{settings.MODEL_KEY[:6]}...{settings.MODEL_KEY[-4:]}" if len(settings.MODEL_KEY) > 10 else "***"
    print(f"\033[90m[Config] Model: {settings.MODEL_NAME} | URL: {settings.MODEL_URL} | Key: {masked_key}\033[0m")
else:
    print(f"\033[33m[Warning] MODEL_KEY is empty. Model: {settings.MODEL_NAME}\033[0m")
