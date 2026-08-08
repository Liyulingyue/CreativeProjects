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


def _load_json(path: Path, default=None):
    if not path.exists():
        return default if default is not None else {}
    with open(path, "r", encoding="utf-8") as f:
        return json.load(f)


def _save_json(path: Path, data):
    path.parent.mkdir(parents=True, exist_ok=True)
    with open(path, "w", encoding="utf-8") as f:
        json.dump(data, f, indent=2, ensure_ascii=False)


class AppState:
    def __init__(self):
        self._settings: AppSettings = AppSettings()
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
