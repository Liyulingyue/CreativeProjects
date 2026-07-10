#!/usr/bin/env python3
"""End-to-end smoke test for ZooGuide.

Verifies:
  1. /api/health returns 200
  2. /api/venues returns 23 venues
  3. /api/plan returns a valid route (LLM or fallback)
  4. /api/replan returns a valid adjusted route
  5. /api/checkin records and reads back
  6. /api/nearest returns closest venues
  7. /api/photo-evaluate accepts an image and returns evaluation
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

        # 6. nearest
        r = c.get(f"{BASE}/api/nearest?lat=32.1030&lon=118.8100&top_k=3")
        d5 = r.json()
        failures += check("nearest", r.status_code == 200 and len(d5.get("results", [])) >= 3,
                          f"top1={d5.get('results', [{}])[0].get('name', '?')}")

        # 7. photo-evaluate (with generated test image)
        import io
        from PIL import Image
        img = Image.new("RGB", (320, 240), (200, 220, 200))
        buf = io.BytesIO()
        img.save(buf, format="JPEG")
        buf.seek(0)
        r = c.post(f"{BASE}/api/photo-evaluate", files={"file": ("test.jpg", buf.read(), "image/jpeg")})
        d6 = r.json()
        failures += check("photo-evaluate", r.status_code == 200 and d6.get("evaluation_id"),
                          f"animal_guess={d6.get('animal_guess', '?')[:20]}, badge={d6.get('badge', '?')}")

        # 8. auth: register
        import time as _t
        uname = f"e2e_{int(_t.time())}"
        r = c.post(f"{BASE}/api/auth/register", json={"username": uname, "password": "test1234"})
        d7 = r.json()
        failures += check("auth register", r.status_code == 200 and d7.get("token"),
                          f"user={d7.get('user', {}).get('username', '?')}")
        token = d7.get("token", "")

        if token:
            auth = {"Authorization": f"Bearer {token}"}
            # 9. auth/me
            r = c.get(f"{BASE}/api/auth/me", headers=auth)
            failures += check("auth me", r.status_code == 200 and r.json().get("username") == uname)
            # 10. login + me again
            r = c.post(f"{BASE}/api/auth/login", json={"username": uname, "password": "test1234"})
            d8 = r.json()
            failures += check("auth login", r.status_code == 200 and d8.get("token"))
            # 10b. authenticated checkin (must come BEFORE /me/summary)
            r = c.post(f"{BASE}/api/checkin", json={"venue_id": "koala"}, headers=auth)
            failures += check("auth checkin", r.status_code == 200)
            # 11. /me/summary
            r = c.get(f"{BASE}/api/me/summary", headers=auth)
            d9 = r.json()
            failures += check("me/summary", r.status_code == 200 and d9.get("stats", {}).get("checkins_count", 0) >= 1,
                              f"checkins={d9.get('stats', {}).get('checkins_count', 0)}")
            # 12. /me/checkins
            r = c.get(f"{BASE}/api/me/checkins", headers=auth)
            failures += check("me/checkins", r.status_code == 200 and isinstance(r.json().get("checkins"), list))
            # 13. duplicate username
            r = c.post(f"{BASE}/api/auth/register", json={"username": uname, "password": "test1234"})
            failures += check("register dup rejected", r.status_code == 409)
            # 14. wrong password
            r = c.post(f"{BASE}/api/auth/login", json={"username": uname, "password": "wrong"})
            failures += check("login wrong password", r.status_code == 401)
            # 15. unauthenticated /me/summary
            r = c.get(f"{BASE}/api/me/summary")
            failures += check("me/summary requires auth", r.status_code == 401)

    print()
    if failures == 0:
        print("🎉 All checks passed.")
        return 0
    else:
        print(f"❌ {failures} check(s) failed.")
        return 1


if __name__ == "__main__":
    sys.exit(main())