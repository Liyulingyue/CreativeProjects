"""Walking minutes matrix between venues.

Strategy:
  1. Use precomputed walking_matrix from venues.json (OSM path network) if available
  2. Fall back to haversine × path_multiplier for missing pairs
"""

from __future__ import annotations

import math
from typing import Optional

from .data_loader import get_all_venue_dicts, get_venue_dict_by_id, get_meta


def _get_gates() -> dict:
    meta = get_meta()
    gates_cfg = meta.get("gates", {})
    return {k: (v["lat"], v["lon"]) for k, v in gates_cfg.items() if "lat" in v and "lon" in v}


def _get_walking_params() -> tuple[float, float]:
    meta = get_meta()
    walking = meta.get("walking", {})
    return walking.get("path_multiplier", 1.5), walking.get("walking_speed_ms", 1.0)


def _get_precomputed_matrix() -> Optional[dict]:
    return get_meta().get("walking_matrix")


def _get_gate_matrix() -> Optional[dict]:
    return get_meta().get("gate_walking_matrix")


def haversine_m(lat1: float, lon1: float, lat2: float, lon2: float) -> float:
    R = 6371000.0
    phi1, phi2 = math.radians(lat1), math.radians(lat2)
    dphi = math.radians(lat2 - lat1)
    dlam = math.radians(lon2 - lon1)
    a = math.sin(dphi / 2) ** 2 + math.cos(phi1) * math.cos(phi2) * math.sin(dlam / 2) ** 2
    return 2 * R * math.asin(math.sqrt(a))


def _coord(venue_id: str, gate: Optional[str] = None) -> Optional[tuple[float, float]]:
    if gate:
        return _get_gates().get(gate)
    v = get_venue_dict_by_id(venue_id)
    if v and "lat" in v and "lon" in v:
        return (v["lat"], v["lon"])
    return None


def get_entry_venue_minutes(gate: str, venue_id: str) -> int:
    gm = _get_gate_matrix()
    if gm and gate in gm and venue_id in gm[gate]:
        return gm[gate][venue_id]
    g = _coord(None, gate)
    v = _coord(venue_id)
    if not g or not v:
        return 25
    mult, speed = _get_walking_params()
    d = haversine_m(g[0], g[1], v[0], v[1]) * mult
    return max(1, round(d / speed / 60))


def get_inter_venue_minutes(a: str, b: str) -> int:
    if a == b:
        return 0
    pm = _get_precomputed_matrix()
    if pm and a in pm and b in pm[a]:
        return pm[a][b]
    va = _coord(a)
    vb = _coord(b)
    if not va or not vb:
        return 8
    mult, speed = _get_walking_params()
    d = haversine_m(va[0], va[1], vb[0], vb[1]) * mult
    return max(1, round(d / speed / 60))


def build_walking_matrix(venue_ids: list[str]) -> dict:
    pm = _get_precomputed_matrix()
    if pm:
        matrix: dict[str, dict[str, int]] = {}
        for a in venue_ids:
            matrix[a] = {}
            for b in venue_ids:
                if a == b:
                    matrix[a][b] = 0
                elif a in pm and b in pm[a]:
                    matrix[a][b] = pm[a][b]
                else:
                    matrix[a][b] = get_inter_venue_minutes(a, b)
        return matrix
    matrix = {}
    for a in venue_ids:
        matrix[a] = {}
        for b in venue_ids:
            matrix[a][b] = get_inter_venue_minutes(a, b)
    return matrix
