"""Rule engine: hard constraints before LLM is invoked.

Inputs: UserPreference + all venues
Outputs: filtered candidate list + per-candidate score
"""

from __future__ import annotations

from datetime import datetime, timedelta

from .data_loader import get_all_venue_dicts
from .models import UserPreference


# Interest -> venue themes/tags/animals mapping
INTEREST_MAP = {
    "panda": {"themes": ["中国本土"], "animals": ["大熊猫"], "tags": ["明星动物"]},
    "ape": {"themes": ["亚洲", "非洲"], "tags": ["灵长类"]},
    "cat": {"animals": ["东北虎", "孟加拉虎", "豹", "豹猫", "猞猁", "薮猫", "狞猫", "美洲豹"], "tags": ["猫科", "大型猫科"]},
    "bird": {"tags": ["鸟类"]},
    "australian": {"themes": ["澳洲", "异域"], "animals": ["考拉", "袋鼠", "鸸鹋", "食火鸡"]},
    "african": {"themes": ["非洲"], "animals": ["犀牛", "长颈鹿", "细尾獴"]},
    "local": {"themes": ["中国本土"]},
    "exotic": {"themes": ["异域", "非洲", "澳洲"]},
    "kids_favorite": {"tags": ["亲子", "明星动物", "网红", "2025新馆"]},
}


def _venue_matches_interest(venue: dict, interest: str) -> bool:
    spec = INTEREST_MAP.get(interest, {})
    if any(t in venue.get("tags", []) for t in spec.get("tags", [])):
        return True
    if any(t in venue.get("themes", []) for t in spec.get("themes", [])):
        return True
    if any(a in venue.get("animals", []) for a in spec.get("animals", [])):
        return True
    return False


def _score_venue(venue: dict, pref: UserPreference) -> float:
    """Higher = better fit. Pure rule-based."""
    score = 0.0
    tags = set(venue.get("tags", []))
    themes = set(venue.get("themes", []))

    # Must-see venues always score high
    if venue.get("must_see"):
        score += 30

    # Interest matching
    if pref.animal_interests:
        matches = sum(1 for i in pref.animal_interests if _venue_matches_interest(venue, i))
        score += matches * 15

    # Kid friendly bonus
    if pref.with_kids:
        score += venue.get("kid_friendly", 0) * 3

    # Sun tolerance: prefer shaded if user is sun-sensitive
    if pref.sun_tolerance <= 2 and venue.get("shaded"):
        score += 10
    if pref.sun_tolerance >= 4 and not venue.get("shaded"):
        score += 2

    # Stamina: prefer rest spots if low stamina
    if pref.stamina <= 2 and venue.get("rest_spots"):
        score += 8

    # Hiking: penalize hills if not willing
    if not pref.willing_to_hike and "坡度大" in tags:
        score -= 15

    # Photo op is universal bonus for younger visitors
    score += venue.get("photo_op", 3) * 1.5

    # Popular (网红 / 2025新馆) for trendy interests
    if pref.party_type in {"solo", "couple"} and ("网红" in tags or "2025新馆" in tags):
        score += 6

    return score


def _is_hard_excluded(venue: dict, pref: UserPreference) -> bool:
    tags = set(venue.get("tags", []))

    # Closed venues (e.g. 周一二闭馆)
    if "周一二闭馆" in tags:
        return True

    # Strongly anti-hill filter when not willing
    if not pref.willing_to_hike and "坡度大" in tags:
        return True

    # Barely any time
    if pref.available_hours <= 1.0:
        return venue.get("recommended_visit_minutes", 20) > 25

    return False


def _venue_open_at(venue: dict, visit_dt: datetime) -> tuple[bool, str]:
    """Check if venue is open at visit_dt. Returns (open, reason_if_closed)."""
    open_str = venue.get("open_time", "08:30")
    close_str = venue.get("close_time", "16:30")
    # Handle "09:00-16:30" format from 大壮观阁
    if "-" in open_str:
        open_str = open_str.split("-")[0]
    try:
        oh, om = map(int, open_str.split(":"))
        ch, cm = map(int, close_str.split(":"))
    except Exception:
        return True, ""
    open_dt = visit_dt.replace(hour=oh, minute=om, second=0, microsecond=0)
    close_dt = visit_dt.replace(hour=ch, minute=cm, second=0, microsecond=0)
    if visit_dt < open_dt:
        return False, f"今天 {open_str} 才开"
    if visit_dt > close_dt:
        return False, f"今天 {close_str} 已闭馆"
    return True, ""


def filter_by_hours(
    candidates: list[dict],
    pref: UserPreference,
    arrival_dt: datetime,
    visit_minutes_per_venue: int = 20,
) -> tuple[list[dict], list[dict]]:
    """Filter (or mark) venues that would be closed when user arrives.

    If strict_hours=True, hard-exclude.
    Returns (kept, warned).
    """
    kept = []
    warned = []
    cur_time = arrival_dt
    for v in candidates:
        open_ok, reason = _venue_open_at(v, cur_time)
        if open_ok:
            kept.append(v)
        else:
            if pref.strict_hours:
                # hard exclude
                continue
            v_copy = dict(v)
            v_copy["_open_warning"] = reason
            warned.append(v_copy)
        # advance time as if visiting
        cur_time = cur_time + timedelta(minutes=visit_minutes_per_venue + 5)
    return kept, warned


def filter_and_rank(pref: UserPreference) -> list[dict]:
    """Return candidates ranked by score, with score attached."""
    venues = get_all_venue_dicts()
    scored: list[tuple[float, dict]] = []
    for v in venues:
        if _is_hard_excluded(v, pref):
            continue
        s = _score_venue(v, pref)
        scored.append((s, v))
    scored.sort(key=lambda x: x[0], reverse=True)

    # Apply operating hours filter (soft by default)
    try:
        hh, mm = map(int, pref.start_time.split(":"))
        arrival = datetime.now().replace(hour=hh, minute=mm, second=0, microsecond=0)
    except Exception:
        arrival = datetime.now().replace(hour=9, minute=0, second=0, microsecond=0)
    if scored:
        candidate_dicts = []
        for s, v in scored:
            v_copy = dict(v)
            v_copy["_score"] = round(s, 1)
            candidate_dicts.append(v_copy)
        kept, warned = filter_by_hours(candidate_dicts, pref, arrival)
        # merge: kept first, warned after (kept already score-sorted)
        out = kept + warned
    else:
        out = []

    return out


def max_stops_by_time(available_hours: float) -> int:
    """Rough cap based on available time."""
    if available_hours <= 1.5:
        return 2
    if available_hours <= 2.5:
        return 4
    if available_hours <= 4:
        return 6
    if available_hours <= 6:
        return 9
    return 12