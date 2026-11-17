"""The name search stops after a while and says so."""
from pathlib import Path

from tests.conftest import client, get_storage_root


def test_a_search_that_gave_up_says_so(monkeypatch):
    import importlib

    search_module = importlib.import_module("app.routers.search")
    monkeypatch.setattr(search_module, "MAX_SEARCH_ENTRIES", 3)
    root = get_storage_root()
    for i in range(10):
        (root / f"文件{i}.txt").write_text("x", encoding="utf-8")

    res = client.get("/api/search/query", params={"q": "文件"})

    assert res.status_code == 200
    body = res.json()
    assert body["truncated"] is True
    assert body["total"] <= 3


def test_a_search_that_looked_everywhere_does_not_say_it_gave_up(monkeypatch):
    import importlib

    search_module = importlib.import_module("app.routers.search")
    monkeypatch.setattr(search_module, "MAX_SEARCH_ENTRIES", 1000)
    root = get_storage_root()
    for i in range(10):
        (root / f"文件{i}.txt").write_text("x", encoding="utf-8")

    res = client.get("/api/search/query", params={"q": "文件"})

    assert res.status_code == 200
    assert res.json()["truncated"] is False
    assert res.json()["total"] == 10
