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


# Universe of warning tips shown on every route
UNIVERSAL_WARNINGS: list[str] = [
    "园内禁止投喂动物，请勿翻越护栏",
    "请勿使用闪光灯，未经允许不得使用无人机",
    "宠物、易燃易爆物品及危险品禁止带入",
    "南门新区地形高差较大，部分区域需注意脚下",
]


def has_valid_llm_config() -> bool:
    return bool(API_KEY) and USE_LLM