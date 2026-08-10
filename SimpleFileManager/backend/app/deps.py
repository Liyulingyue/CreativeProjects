import json
import os
import time
from pathlib import Path
from typing import Optional

from dotenv import load_dotenv
from .models import AppSettings, IndexStats, IndexStatus

load_dotenv()

BASE_DIR = Path(__file__).resolve().parent.parent
DATA_DIR = BASE_DIR / "data"
DATA_DIR.mkdir(exist_ok=True)

SETTINGS_FILE = DATA_DIR / "settings.json"
INDEX_STATS_FILE = DATA_DIR / "index_stats.json"

EMBEDDING_DIM_MAP = {
    "text-embedding-3-small": 1536,
    "text-embedding-3-large": 3072,
    "text-embedding-ada-002": 1538,
}


def _detect_embedding_dim(model: str, api_key: str, base_url: str) -> int:
    try:
        import httpx
        headers = {"Authorization": f"Bearer {api_key}"} if api_key else {}
        resp = httpx.post(
            base_url,
            headers=headers,
            json={"input": "test", "model": model},
            timeout=10,
        )
        if resp.status_code == 200:
            data = resp.json()
            embedding = data.get("data", [{}])[0]
            embedding_vector = embedding.get("embedding", [])
            if embedding_vector:
                return len(embedding_vector)
        for known_model, dim in EMBEDDING_DIM_MAP.items():
            if model.startswith(known_model):
                return dim
        return 1536
    except Exception:
        for known_model, dim in EMBEDDING_DIM_MAP.items():
            if model.startswith(known_model):
                return dim
        return 1536


def _load_json(path: Path, default=None):
    if not path.exists():
        return default if default is not None else {}
    with open(path, "r", encoding="utf-8") as f:
        return json.load(f)


def _save_json(path: Path, data):
    path.parent.mkdir(parents=True, exist_ok=True)
    with open(path, "w", encoding="utf-8") as f:
        json.dump(data, f, indent=2, ensure_ascii=False)


def _get_default_settings() -> AppSettings:
    embedding_dim_str = os.getenv("EMBEDDING_DIM", "AUTO")
    embedding_api_key = os.getenv("EMBEDDING_API_KEY", "")
    embedding_base_url = os.getenv("EMBEDDING_BASE_URL", "https://api.minimaxi.com/v1")
    embedding_model = os.getenv("EMBEDDING_MODEL", "text-embedding-3-small")

    if embedding_dim_str == "AUTO":
        embedding_dim_str = str(_detect_embedding_dim(embedding_model, embedding_api_key, embedding_base_url))

    return AppSettings(
        llm_api_key=os.getenv("LLM_API_KEY", ""),
        llm_base_url=os.getenv("LLM_BASE_URL", "https://api.minimaxi.com/v1"),
        llm_model=os.getenv("LLM_MODEL", "gpt-4o-mini"),
        embedding_api_key=embedding_api_key,
        embedding_base_url=embedding_base_url,
        embedding_model=embedding_model,
        embedding_dim=embedding_dim_str,
        index_interval=int(os.getenv("INDEX_INTERVAL", "300")),
        storage_path=os.getenv("STORAGE_PATH", "./data"),
    )


class AppState:
    def __init__(self):
        self._settings: AppSettings = _get_default_settings()
        self._index_stats: IndexStats = IndexStats(
            total_files=0, total_dirs=0, indexed_files=0, indexed_dirs=0
        )
        self._index_status: IndexStatus = IndexStatus(is_indexing=False, progress=0.0)
        self._load()

    def _load(self):
        settings_data = _load_json(SETTINGS_FILE, None)
        if settings_data:
            self._settings = AppSettings(**settings_data)

        stats_data = _load_json(INDEX_STATS_FILE, None)
        if stats_data:
            self._index_stats = IndexStats(**stats_data)

    def _persist_settings(self):
        _save_json(SETTINGS_FILE, self._settings.model_dump())

    def _persist_index_stats(self):
        _save_json(INDEX_STATS_FILE, self._index_stats.model_dump())

    def get_settings(self) -> AppSettings:
        return self._settings

    def update_settings(self, updates: dict) -> AppSettings:
        for k, v in updates.items():
            if hasattr(self._settings, k):
                setattr(self._settings, k, v)
        self._persist_settings()
        return self._settings

    def get_index_stats(self) -> IndexStats:
        return self._index_stats

    def update_index_stats(self, updates: dict) -> IndexStats:
        for k, v in updates.items():
            if hasattr(self._index_stats, k):
                setattr(self._index_stats, k, v)
        self._persist_index_stats()
        return self._index_stats

    def get_index_status(self) -> IndexStatus:
        return self._index_status

    def set_indexing(self, is_indexing: bool, progress: float = 0.0, current_path: Optional[str] = None):
        self._index_status = IndexStatus(
            is_indexing=is_indexing,
            progress=progress,
            current_path=current_path
        )


state = AppState()
