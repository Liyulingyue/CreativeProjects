from __future__ import annotations

import json
from functools import lru_cache
from pathlib import Path

from .models import Facility, Venue

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


def get_all_facilities() -> list[Facility]:
    return [Facility(**f) for f in _load_raw().get("facilities", [])]


def get_facility_by_id(fid: str) -> Facility | None:
    for f in get_all_facilities():
        if f.id == fid:
            return f
    return None


def get_facility_categories() -> list[str]:
    seen = []
    for f in _load_raw().get("facilities", []):
        cat = f.get("category", "")
        if cat and cat not in seen:
            seen.append(cat)
    return seen