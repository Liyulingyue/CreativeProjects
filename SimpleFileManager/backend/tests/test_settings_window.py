"""Settings endpoint: the window is checked, other settings are not."""
from fastapi.testclient import TestClient
from app.main import app

client = TestClient(app)

def test_a_broken_picture_window_is_refused():
    r = client.post("/api/settings", json={"image_index_window": "09:13-09:13:56"})
    assert r.status_code == 400, r.text
    assert "HH:MM-HH:MM" in r.json()["detail"]

def test_a_sane_window_is_accepted():
    r = client.post("/api/settings", json={"image_index_window": "09:00-09:30"})
    assert r.status_code == 200, r.text
    assert r.json()["image_index_window"] == "09:00-09:30"

def test_other_settings_are_not_touched_by_the_check():
    r = client.post("/api/settings", json={"trash_retention_days": 30, "theme": "dark"})
    assert r.status_code == 200, r.text
    d = r.json()
    assert d["trash_retention_days"] == 30
    assert d["theme"] == "dark"

def test_all_day_is_allowed():
    r = client.post("/api/settings", json={"image_index_window": "all"})
    assert r.status_code == 200, r.text
