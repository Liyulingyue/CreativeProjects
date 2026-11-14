"""Tests for the holes the first round of review found."""

import threading
import time

import pytest

from app.image_search import SearchUnavailable, search_by_image, search_by_text
from app.image_store import ImageVectorStore


@pytest.fixture
def store(tmp_path, monkeypatch):
    from app.deps import get_storage_root

    root = get_storage_root()
    for name in ("photos/apple.jpg", "photos/cat.jpg"):
        path = root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(b"\xff\xd8\xff\xe0" + b"\x00" * 32)
    s = ImageVectorStore(str(tmp_path / "vectors"), dim=4)
    s.add([1.0, 0.0, 0.0, 0.0], "photos/apple.jpg")
    s.add([0.0, 1.0, 0.0, 0.0], "photos/cat.jpg")
    monkeypatch.setattr("app.image_search.get_image_store", lambda *a, **k: s)
    return s


class FakeEmbedder:
    def embed_text(self, text, prefix=""):
        return [1.0, 0.0, 0.0, 0.0]

    def embed_image(self, path):
        return [1.0, 0.0, 0.0, 0.0]


def test_a_picture_the_caller_may_not_open_is_not_returned(store, monkeypatch):
    """The index holds everything; the answer holds only what is open."""
    monkeypatch.setattr("app.image_search.get_image_embedder", lambda: FakeEmbedder())
    monkeypatch.setattr("app.deps.can_access", lambda target, mode="read": False)

    assert search_by_text("红色的水果") == []


def test_the_readable_ones_still_come_through(store, monkeypatch):
    monkeypatch.setattr("app.image_search.get_image_embedder", lambda: FakeEmbedder())
    seen = []

    def can_access(target, mode="read"):
        seen.append(str(target))
        return "apple" in str(target)

    monkeypatch.setattr("app.deps.can_access", can_access)
    results = search_by_text("红色的水果")
    assert [r["path"] for r in results] == ["photos/apple.jpg"]
    assert len(seen) == 2


def test_a_path_outside_the_storage_root_is_refused(store, monkeypatch):
    """A path from the model must not reach outside the storage root."""
    monkeypatch.setattr("app.image_search.get_image_embedder", lambda: FakeEmbedder())
    with pytest.raises(ValueError):
        search_by_image("../../../etc/passwd.jpg")


def test_a_path_outside_the_root_never_reaches_the_model(store, monkeypatch):
    calls = []

    class Spy:
        def embed_image(self, path):
            calls.append(str(path))
            return [1.0, 0.0, 0.0, 0.0]

    monkeypatch.setattr("app.image_search.get_image_embedder", lambda: Spy())
    with pytest.raises(ValueError):
        search_by_image("../outside.jpg")
    assert calls == []


def test_two_threads_may_create_the_table_at_the_same_time(tmp_path):
    """The first use of the index can happen twice at once."""
    errors = []
    made = []

    def create():
        try:
            made.append(ImageVectorStore(str(tmp_path / "vectors"), dim=4))
        except Exception as e:  # noqa: BLE001
            errors.append(e)

    threads = [threading.Thread(target=create) for _ in range(4)]
    for t in threads:
        t.start()
    for t in threads:
        t.join(10)
    assert errors == [], errors
    assert len(made) == 4


def test_the_table_is_cached_per_path(tmp_path):
    from app.image_store import get_image_store
    a = get_image_store(str(tmp_path / "one"))
    b = get_image_store(str(tmp_path / "one"))
    c = get_image_store(str(tmp_path / "two"))
    assert a is b
    assert a is not c
