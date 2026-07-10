"""Walking minutes matrix (estimates between venues).

Numbers are conservative estimates in MINUTES, not meters.
Designed for the topology:

  北门 ── 大红山片区(熊猫/虎/猫科) ─┐
                                    ├── 中心广场 ── 放牛山(考拉/象/猩猩)
  东门 ── 高黎贡/冈瓦纳(东侧)   ─┘        │
                                          └── 南门新区(非洲/澳洲/唐家河/大猩猩)

Distances are derived from公开资料:
  - 大熊猫馆离北门 ≤ 300m
  - 南门新区游览线 1.6km
  - 三大门串联起三片区

For 距离 estimates within 1000m assume 1.2m/s walking pace.
"""

from __future__ import annotations

# gate adjacency
WALK_FROM_GATE = {
    "north": {
        "panda": 5,
        "meerkat": 8,
        "wolf": 12,
        "monkey_mountain": 14,
        "bear": 16,
        "tiger": 18,
        "giraffe": 22,
        "china_cat": 24,
        "cat_planet": 25,
        "dazhuangguange": 28,
        "lemur": 30,
        "rhino": 32,
        "hornbill": 8,
        "crane": 10,
        "orangutan": 25,
        "asian_elephant": 28,
        "koala": 30,
        "asian_primates": 32,
        "red_panda": 33,
        "kangaroo": 35,
        "gorilla": 36,
        "tangjiahe": 38,
        "gonwana": 40,
    },
    "south": {
        "gorilla": 3,
        "kangaroo": 5,
        "lemur": 7,
        "rhino": 9,
        "tangjiahe": 10,
        "gonwana": 12,
        "koala": 15,
        "asian_elephant": 17,
        "orangutan": 18,
        "red_panda": 20,
        "asian_primates": 22,
        "giraffe": 25,
        "panda": 30,
        "tiger": 33,
        "cat_planet": 35,
        "china_cat": 36,
        "bear": 38,
        "monkey_mountain": 40,
        "wolf": 42,
        "meerkat": 44,
        "hornbill": 25,
        "crane": 27,
        "dazhuangguange": 38,
    },
    "east": {
        "gonwana": 5,
        "tangjiahe": 8,
        "rhino": 10,
        "lemur": 12,
        "kangaroo": 14,
        "gorilla": 16,
        "giraffe": 18,
        "koala": 22,
        "asian_elephant": 24,
        "orangutan": 25,
        "red_panda": 26,
        "asian_primates": 27,
        "tiger": 30,
        "panda": 32,
        "hornbill": 14,
        "crane": 16,
    },
}


def get_entry_venue_minutes(gate: str, venue_id: str) -> int:
    return WALK_FROM_GATE.get(gate, WALK_FROM_GATE["north"]).get(venue_id, 25)


# Inter-venue walking matrix, minutes. Conservative estimates.
# Only defined for meaningful adjacent pairs.
INTER_VENUE = {
    ("panda", "meerkat"): 3,
    ("meerkat", "wolf"): 4,
    ("wolf", "monkey_mountain"): 3,
    ("monkey_mountain", "bear"): 3,
    ("bear", "tiger"): 3,
    ("tiger", "giraffe"): 4,
    ("giraffe", "china_cat"): 2,
    ("china_cat", "cat_planet"): 1,
    ("giraffe", "lemur"): 5,
    ("giraffe", "asian_elephant"): 4,
    ("asian_elephant", "koala"): 3,
    ("asian_elephant", "orangutan"): 2,
    ("orangutan", "asian_primates"): 3,
    ("asian_primates", "red_panda"): 2,
    ("lemur", "rhino"): 3,
    ("lemur", "kangaroo"): 3,
    ("rhino", "kangaroo"): 4,
    ("kangaroo", "gorilla"): 4,
    ("gorilla", "tangjiahe"): 3,
    ("tangjiahe", "gonwana"): 3,
    ("hornbill", "crane"): 2,
    ("panda", "hornbill"): 4,
    ("giraffe", "gonwana"): 8,
    ("giraffe", "kangaroo"): 6,
    ("lemur", "gorilla"): 4,
    ("hornbill", "tiger"): 6,
}


def get_inter_venue_minutes(a: str, b: str) -> int:
    if a == b:
        return 0
    if (a, b) in INTER_VENUE:
        return INTER_VENUE[(a, b)]
    if (b, a) in INTER_VENUE:
        return INTER_VENUE[(b, a)]
    return 8


def build_walking_matrix(venue_ids: list[str]) -> dict:
    matrix: dict[str, dict[str, int]] = {}
    for a in venue_ids:
        matrix[a] = {}
        for b in venue_ids:
            matrix[a][b] = get_inter_venue_minutes(a, b)
    return matrix