"""Tests for the picture search: text to pictures, and picture to pictures."""

import pytest

from app.image_search import SearchUnavailable, search_by_image, search_by_text
from app.image_store import ImageVectorStore


class FakeEmbedder:
    def __init__(self, vector=None):
        self.vector = vector or [1.0, 0.0, 0.0, 0.0]
        self.calls = []

    def embed_text(self, text, prefix=""):
        self.calls.append(("text", text, prefix))
        return list(self.vector)

    def embed_image(self, path):
        self.calls.append(("image", str(path)))
        return list(self.vector)


@pytest.fixture
def store(tmp_path, monkeypatch):
    s = ImageVectorStore(str(tmp_path / "vectors"), dim=4)
    s.add([1.0, 0.0, 0.0, 0.0], "photos/apple.jpg")
    s.add([0.0, 1.0, 0.0, 0.0], "photos/cat.jpg")
    monkeypatch.setattr("app.image_search._image_store", lambda: s)
    return s


def test_text_finds_the_closest_picture(store, monkeypatch):
    embedder = FakeEmbedder()
    monkeypatch.setattr("app.image_search.get_image_embedder", lambda: embedder)

    results = search_by_text("一个红色的苹果")
    assert results[0]["path"] == "photos/apple.jpg"
    assert results[0]["score"] > 0
    assert embedder.calls[0][0] == "text"


def test_the_query_carries_the_prefix(store, monkeypatch):
    embedder = FakeEmbedder()
    monkeypatch.setattr("app.image_search.get_image_embedder", lambda: embedder)
    search_by_text("海边日落")
    assert embedder.calls[0][2] == "Query: "


def test_an_empty_query_is_refused(store, monkeypatch):
    monkeypatch.setattr("app.image_search.get_image_embedder", lambda: FakeEmbedder())
    with pytest.raises(ValueError):
        search_by_text("   ")


def test_a_busy_server_refuses_instead_of_queueing(store, monkeypatch):
    """The cores are taken by something else -- the model, or a build.

    The search gives up rather than queueing: someone waiting for a picture
    does not want to be behind a build.
    """
    monkeypatch.setattr("app.image_search.SEARCH_LOCK_TIMEOUT", 0.05)
    monkeypatch.setattr("app.image_search.get_image_embedder", lambda: FakeEmbedder())

    import threading
    from app.cpu_pool import get_big_pool

    held = threading.Event()
    released = threading.Event()

    def holder():
        with get_big_pool().hold():
            held.set()
            released.wait(5)

    t = threading.Thread(target=holder, daemon=True)
    t.start()
    assert held.wait(5)
    try:
        with pytest.raises(SearchUnavailable) as err:
            search_by_text("海边日落")
    finally:
        released.set()
        t.join(5)
    assert "不可用" in str(err.value)


def test_the_search_is_released_afterwards(store, monkeypatch):
    monkeypatch.setattr("app.image_search.get_image_embedder", lambda: FakeEmbedder())
    from app.cpu_pool import get_big_pool
    search_by_text("海边日落")
    # If the lock had leaked, this would block.
    with get_big_pool().try_hold(timeout=0.1) as got:
        assert got is True


def test_a_picture_finds_its_neighbours(store, monkeypatch, tmp_path):
    from app.deps import get_storage_root
    root = get_storage_root()
    (root / "photos").mkdir(parents=True, exist_ok=True)
    (root / "photos" / "cat.jpg").write_bytes(b"\xff\xd8\xff\xd9")

    # The query picture encodes to the same vector as the cat in the index.
    embedder = FakeEmbedder([0.0, 1.0, 0.0, 0.0])
    monkeypatch.setattr("app.image_search.get_image_embedder", lambda: embedder)

    results = search_by_image("photos/cat.jpg")
    assert results[0]["path"] == "photos/cat.jpg"
    assert results[0].get("is_source") is True


def test_a_file_that_is_not_a_picture_is_refused(store, monkeypatch):
    from app.deps import get_storage_root
    root = get_storage_root()
    (root / "notes.txt").write_text("hello")

    monkeypatch.setattr("app.image_search.get_image_embedder", lambda: FakeEmbedder())
    with pytest.raises(ValueError):
        search_by_image("notes.txt")


def test_an_empty_index_gives_nothing(store, monkeypatch, tmp_path):
    empty = ImageVectorStore(str(tmp_path / "empty"), dim=4)
    monkeypatch.setattr("app.image_search._image_store", lambda: empty)
    monkeypatch.setattr("app.image_search.get_image_embedder", lambda: FakeEmbedder())
    assert search_by_text("海边日落") == []


def test_a_dead_jina_service_says_so(store, monkeypatch):
    """A machine without the picture model must not look like an empty index."""
    from app.image_embed import JinaError

    class Dead:
        def embed_text(self, text, prefix=""):
            raise JinaError("无法连接 jina 服务 http://127.0.0.1:9014: Connection refused")

    monkeypatch.setattr("app.image_search.get_image_embedder", lambda: Dead())
    with pytest.raises(SearchUnavailable) as err:
        search_by_text("海边日落")
    assert "不可用" in str(err.value)
