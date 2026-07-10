from __future__ import annotations

import json
from functools import lru_cache
from pathlib import Path

from .models import Venue

DATA_DIR = Path(__file__).resolve().parent.parent / "data"


@lru_cache(maxsize=1)
def _load_raw() -> dict:
    with (DATA_DIR / "venues.json").open(encoding="utf-8") as f:
        return json.load(f)


def get_meta() -> dict:
    return _load_raw()["meta"]


def get_all_venues() -> list[Venue]:
    return [Venue(**v) for v in _load_raw()["venues"]]


def get_venue_by_id(venue_id: str) -> Venue | None:
    for v in get_all_venues():
        if v.id == venue_id:
            return v
    return None


def get_venue_dict_by_id(venue_id: str) -> dict | None:
    for v in _load_raw()["venues"]:
        if v["id"] == venue_id:
            return v
    return None


def get_all_venue_dicts() -> list[dict]:
    return _load_raw()["venues"]


def get_tags_glossary() -> dict:
    return _load_raw().get("tags_glossary", {})