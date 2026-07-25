from app.planner import (
    _fallback_summary,
    _fallback_narration,
    _fallback_general_tips,
    plan_route,
)
from app.models import UserPreference, PlanRequest


def test_fallback_summary():
    venues = [
        {"name": "大熊猫馆"},
        {"name": "考拉馆"},
    ]
    prefs = UserPreference(
        available_hours=3.0, party_type="solo", with_kids=False,
        stamina=3, sun_tolerance=3, willing_to_hike=True,
        animal_interests=["panda"], entry_gate="north", start_time="09:00",
    )
    s = _fallback_summary(venues, prefs, "balanced")
    assert "2" in s or "两" in s
    assert "大熊猫馆" in s


def test_fallback_summary_empty():
    prefs = UserPreference(
        available_hours=0.5, party_type="solo", with_kids=False,
        stamina=1, sun_tolerance=1, willing_to_hike=False,
        animal_interests=[], entry_gate="north", start_time="09:00",
    )
    s = _fallback_summary([], prefs, "balanced")
    assert len(s) > 0


def test_fallback_narration():
    venue = {"name": "大熊猫馆", "animals": ["大熊猫"]}
    prefs = UserPreference(
        available_hours=3.0, party_type="solo", with_kids=False,
        stamina=3, sun_tolerance=3, willing_to_hike=True,
        animal_interests=["panda"], entry_gate="north", start_time="09:00",
    )
    n = _fallback_narration(venue, prefs)
    assert len(n) > 10
    assert "大熊猫" in n


def test_fallback_general_tips():
    prefs = UserPreference(
        available_hours=3.0, party_type="family_young", with_kids=True,
        stamina=2, sun_tolerance=1, willing_to_hike=False,
        animal_interests=[], entry_gate="north", start_time="09:00",
    )
    tips = _fallback_general_tips(prefs)
    assert isinstance(tips, list)
    assert len(tips) >= 1


def test_plan_route_fallback():
    req = PlanRequest(
        available_hours=2.0, party_type="solo", with_kids=False,
        stamina=3, sun_tolerance=3, willing_to_hike=True,
        animal_interests=["panda"], entry_gate="north", start_time="09:00",
    )
    route, used_llm = plan_route(req, force_fast=True)
    assert route is not None
    assert len(route.stops) >= 1
    assert used_llm is False
