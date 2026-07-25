"""Configuration loaded from environment variables."""

from __future__ import annotations

import os
from pathlib import Path

from dotenv import load_dotenv

_ENV_PATH = Path(__file__).resolve().parent.parent / ".env"
load_dotenv(_ENV_PATH)


API_KEY: str = os.getenv("OPENAI_API_KEY", "")
BASE_URL: str = os.getenv("OPENAI_BASE_URL", "https://api.openai.com/v1")
MODEL_NAME: str = os.getenv("OPENAI_MODEL", "gpt-4o-mini")

USE_LLM: bool = os.getenv("USE_LLM", "true").lower() in ("1", "true", "yes")

CHAT_REGEX_FAST_PATH: bool = os.getenv("CHAT_REGEX_FAST_PATH", "false").lower() in ("1", "true", "yes")

CORS_ORIGINS: list[str] = [
    o.strip()
    for o in os.getenv(
        "CORS_ORIGINS",
        "http://localhost:5173,http://127.0.0.1:5173,http://localhost:4173",
    ).split(",")
    if o.strip()
]

HOST: str = os.getenv("HOST", "0.0.0.0")
PORT: int = int(os.getenv("PORT", "8000"))


# Universe of warning tips shown on every route (loaded from meta at runtime)
UNIVERSAL_WARNINGS: list[str] = []


def _load_warnings() -> list[str]:
    try:
        from .data_loader import get_meta
        return get_meta().get("warnings", [])
    except Exception:
        return []


UNIVERSAL_WARNINGS = _load_warnings()


def has_valid_llm_config() -> bool:
    return bool(API_KEY) and USE_LLM