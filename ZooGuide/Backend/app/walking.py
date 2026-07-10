"""Walking minutes matrix (estimates between venues).

Strategy:
  1. Compute haversine distance between venues (using lat/lon)
  2. Apply path multiplier (1.8x for 红山 - hilly, winding paths, not direct)
  3. Use 1.0 m/s walking speed (slower than flat, accounts for stairs/uphill)

红山 is a hilly forest zoo — actual walking paths are NOT direct lines.
Public data: 南门新区游览线 1.6km, 大熊猫离北门 ≤300m.
Our multiplier of 1.8x accounts for non-direct paths, slopes, and stairs.
"""

from __future__ import annotations

import math
from typing import Optional

from .data_loader import get_all_venue_dicts, get_venue_dict_by_id


# Real gates (lat/lon from venues.json meta)
GATES = {
    "north": (32.1035, 118.8100),
    "south": (32.0945, 118.8125),
    "east":  (32.0995, 118.8165),
}

# 红山是山地，路径非直线（参考：北门→大熊猫馆300m官方约5分钟，即~60m/min ≈ 1m/s, 2x 倍 haversine）
PATH_MULTIPLIER = 2.5
WALKING_SPEED_MS = 0.9  # 平地 ~1.2 m/s, 山地降速 ~0.9


def haversine_m(lat1: float, lon1: float, lat2: float, lon2: float) -> float:
    R = 6371000.0
    phi1, phi2 = math.radians(lat1), math.radians(lat2)
    dphi = math.radians(lat2 - lat1)
    dlam = math.radians(lon2 - lon1)
    a = math.sin(dphi / 2) ** 2 + math.cos(phi1) * math.cos(phi2) * math.sin(dlam / 2) ** 2
    return 2 * R * math.asin(math.sqrt(a))


def _coord(venue_id: str, gate: Optional[str] = None) -> Optional[tuple[float, float]]:
    if gate:
        return GATES.get(gate)
    v = get_venue_dict_by_id(venue_id)
    if v and "lat" in v and "lon" in v:
        return (v["lat"], v["lon"])
    return None


def get_entry_venue_minutes(gate: str, venue_id: str) -> int:
    """Minutes from gate to first venue."""
    g = _coord(None, gate)
    v = _coord(venue_id)
    if not g or not v:
        return 25
    d = haversine_m(g[0], g[1], v[0], v[1]) * PATH_MULTIPLIER
    return max(1, round(d / WALKING_SPEED_MS / 60))


def get_inter_venue_minutes(a: str, b: str) -> int:
    """Minutes from venue A to venue B (symmetric)."""
    if a == b:
        return 0
    va = _coord(a)
    vb = _coord(b)
    if not va or not vb:
        return 8
    d = haversine_m(va[0], va[1], vb[0], vb[1]) * PATH_MULTIPLIER
    return max(1, round(d / WALKING_SPEED_MS / 60))


def build_walking_matrix(venue_ids: list[str]) -> dict:
    matrix: dict[str, dict[str, int]] = {}
    for a in venue_ids:
        matrix[a] = {}
        for b in venue_ids:
            matrix[a][b] = get_inter_venue_minutes(a, b)
    return matrix