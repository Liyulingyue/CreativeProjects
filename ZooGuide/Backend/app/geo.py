"""Geo helpers: find nearest venue given lat/lon."""

from __future__ import annotations

import math
from typing import Optional

from .data_loader import get_all_venue_dicts, get_meta


def _get_bbox() -> dict:
    meta = get_meta()
    b = meta.get("bbox")
    if b:
        return {
            "min_lat": b["lat_min"],
            "max_lat": b["lat_max"],
            "min_lon": b["lon_min"],
            "max_lon": b["lon_max"],
        }
    return bbox()


def haversine_m(lat1: float, lon1: float, lat2: float, lon2: float) -> float:
    """Distance in meters between two lat/lon points."""
    R = 6371000.0
    phi1, phi2 = math.radians(lat1), math.radians(lat2)
    dphi = math.radians(lat2 - lat1)
    dlam = math.radians(lon2 - lon1)
    a = math.sin(dphi / 2) ** 2 + math.cos(phi1) * math.cos(phi2) * math.sin(dlam / 2) ** 2
    return 2 * R * math.asin(math.sqrt(a))


def find_nearest_venues(
    lat: float,
    lon: float,
    top_k: int = 3,
    max_distance_m: float = 1500.0,
) -> list[dict]:
    """Return top-k nearest venues by haversine distance."""
    venues = get_all_venue_dicts()
    scored: list[tuple[float, dict]] = []
    for v in venues:
        if "lat" not in v or "lon" not in v:
            continue
        d = haversine_m(lat, lon, v["lat"], v["lon"])
        if d <= max_distance_m:
            scored.append((d, v))
    scored.sort(key=lambda x: x[0])
    out = []
    for d, v in scored[:top_k]:
        v_copy = dict(v)
        v_copy["distance_m"] = round(d, 1)
        out.append(v_copy)
    return out


def is_within_park(lat: float, lon: float) -> bool:
    b = _get_bbox()
    return b["min_lat"] <= lat <= b["max_lat"] and b["min_lon"] <= lon <= b["max_lon"]


def bbox() -> dict:
    venues = get_all_venue_dicts()
    lats = [v["lat"] for v in venues if "lat" in v]
    lons = [v["lon"] for v in venues if "lon" in v]
    if not lats:
        return _get_bbox()
    return {
        "min_lat": min(lats) - 0.001,
        "max_lat": max(lats) + 0.001,
        "min_lon": min(lons) - 0.001,
        "max_lon": max(lons) + 0.001,
    }