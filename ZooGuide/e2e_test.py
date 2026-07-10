#!/usr/bin/env python3
"""End-to-end smoke test for ZooGuide.

Verifies:
  1. /api/health returns 200
  2. /api/venues returns 23 venues
  3. /api/plan returns a valid route (LLM or fallback)
  4. /api/replan returns a valid adjusted route
  5. /api/checkin records and reads back
"""
import json
import sys
import time
from typing import Any

import httpx

BASE = "http://127.0.0.1:8000"


def check(name: str, ok: bool, detail: str = ""):
    sym = "✅" if ok else "❌"
    print(f"  {sym} {name}" + (f" — {detail}" if detail else ""))
    return 0 if ok else 1


def main() -> int:
    failures = 0
    with httpx.Client(timeout=180.0) as c:
        # 1. health
        r = c.get(f"{BASE}/api/health")
        failures += check("health", r.status_code == 200, f"{r.json().get('venue_count', '?')} venues")

        # 2. venues
        r = c.get(f"{BASE}/api/venues")
        d = r.json()
        failures += check("venues", r.status_code == 200 and len(d["venues"]) >= 20, f"{len(d['venues'])} venues")

        # 3. plan: family_young
        prefs: dict[str, Any] = {
            "available_hours": 3.0,
            "party_type": "family_young",
            "with_kids": True,
            "kids_age": 5,
            "stamina": 3,
            "sun_tolerance": 2,
            "willing_to_hike": False,
            "animal_interests": ["panda", "ape", "kids_favorite"],
            "entry_gate": "north",
            "start_time": "09:00",
        }
        t0 = time.time()
        r = c.post(f"{BASE}/api/plan", json=prefs)
        d = r.json()
        dt = time.time() - t0
        failures += check("plan", r.status_code == 200 and len(d.get("stops", [])) >= 2,
                          f"{len(d.get('stops', []))} stops in {dt:.1f}s (llm_used={d.get('llm_used')})")
        if d.get("stops"):
            first = d["stops"][0]
            failures += check("plan has narration", bool(first.get("narration")), f"'{first['narration'][:60]}...'")
            failures += check("plan has timestamps", bool(first.get("arrive_time")) and bool(first.get("leave_time")),
                              f"{first.get('arrive_time')}-{first.get('leave_time')}")

        # 4. replan
        replan_req = {
            "original_route": d,
            "current_venue_id": d["stops"][1]["venue_id"] if len(d["stops"]) > 1 else d["stops"][0]["venue_id"],
            "elapsed_minutes": 60,
            "feedback": "孩子累了太阳也晒了，能少走点吗",
        }
        t0 = time.time()
        r = c.post(f"{BASE}/api/replan", json=replan_req)
        d2 = r.json()
        dt = time.time() - t0
        failures += check("replan", r.status_code == 200 and len(d2.get("stops", [])) >= 1,
                          f"{len(d2.get('stops', []))} new stops in {dt:.1f}s")

        # 5. checkin
        r = c.post(f"{BASE}/api/checkin", json={"venue_id": "panda"})
        d3 = r.json()
        failures += check("checkin", r.status_code == 200 and d3.get("ok"),
                          f"session_id={d3.get('session_id', '?')[:8]}...")
        sid = d3.get("session_id")
        if sid:
            r = c.get(f"{BASE}/api/checkin/{sid}")
            d4 = r.json()
            failures += check("checkin readback", r.status_code == 200 and len(d4.get("checkins", [])) >= 1,
                              f"{len(d4.get('checkins', []))} checkins")

    print()
    if failures == 0:
        print("🎉 All checks passed.")
        return 0
    else:
        print(f"❌ {failures} check(s) failed.")
        return 1


if __name__ == "__main__":
    sys.exit(main())