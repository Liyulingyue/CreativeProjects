from app.rule_engine import (
    INTEREST_MAP,
    filter_and_rank,
    _score_venue,
)
from app.models import UserPreference


def test_interest_map_loaded():
    assert len(INTEREST_MAP) >= 1


def test_filter_and_rank():
    prefs = UserPreference(
        available_hours=3.0, party_type="solo", with_kids=False,
        stamina=3, sun_tolerance=3, willing_to_hike=True,
        animal_interests=["panda"], entry_gate="north", start_time="09:00",
    )
    result = filter_and_rank(prefs)
    assert isinstance(result, list)
    assert len(result) >= 1
    ids = [v["id"] for v in result]
    assert "panda" in ids


def test_score_venue_must_see_boost():
    v_must = {"id": "a", "must_see": True, "tags": ["明星动物"], "kid_friendly": 3, "shaded": True}
    v_not = {"id": "b", "must_see": False, "tags": [], "kid_friendly": 3, "shaded": True}
    prefs = UserPreference(
        available_hours=3.0, party_type="solo", with_kids=False,
        stamina=3, sun_tolerance=3, willing_to_hike=True,
        animal_interests=[], entry_gate="north", start_time="09:00",
    )
    s_must = _score_venue(v_must, prefs)
    s_not = _score_venue(v_not, prefs)
    assert s_must > s_not
