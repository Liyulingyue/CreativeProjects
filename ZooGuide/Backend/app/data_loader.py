from __future__ import annotations

import json
import logging
from functools import lru_cache
from pathlib import Path

from .models import Facility, Venue

DATA_DIR = Path(__file__).resolve().parent.parent / "data"
logger = logging.getLogger(__name__)

_MINIMAL_RAW: dict = {
    "meta": {"name": "动物园", "short_name": "动物园"},
    "venues": [],
    "facilities": [],
}


@lru_cache(maxsize=1)
def _load_raw() -> dict:
    path = DATA_DIR / "venues.json"
    try:
        with path.open(encoding="utf-8") as f:
            data = json.load(f)
        if "meta" not in data or "venues" not in data:
            logger.error("venues.json missing 'meta' or 'venues' key, using fallback")
            return _MINIMAL_RAW
        return data
    except FileNotFoundError:
        logger.error("venues.json not found at %s, using fallback", path)
        return _MINIMAL_RAW
    except json.JSONDecodeError as e:
        logger.error("venues.json invalid JSON: %s, using fallback", e)
        return _MINIMAL_RAW


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


def get_curated_routes() -> list[dict]:
    return _load_raw().get("meta", {}).get("curated_routes", [])


def get_quiz_config() -> dict:
    return _load_raw().get("meta", {}).get("quiz", {})


def get_facility_categories() -> list[str]:
    seen = []
    for f in _load_raw().get("facilities", []):
        cat = f.get("category", "")
        if cat and cat not in seen:
            seen.append(cat)
    return seen