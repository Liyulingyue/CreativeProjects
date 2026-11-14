"""Tests for building the picture index."""

import time

import pytest

from app.image_indexer import ImageIndexer
from app.image_store import ImageVectorStore


class FakeEmbedder:
    def __init__(self):
        self.encoded = []

    def embed_image(self, path):
        self.encoded.append(str(path))
        return [1.0, 0.0, 0.0, 0.0]


def _picture(root, name):
    p = root / name
    p.parent.mkdir(parents=True, exist_ok=True)
    p.write_bytes(b"\xff\xd8\xff\xd9")
    return p


@pytest.fixture
def pictures():
    from app.deps import get_storage_root
    root = get_storage_root()
    _picture(root, "photos/one.jpg")
    _picture(root, "photos/two.png")
    (root / "photos").mkdir(parents=True, exist_ok=True)
    (root / "notes.txt").write_text("not a picture")
    return root


def test_the_index_picks_up_the_pictures(pictures, tmp_path, monkeypatch):
    store = ImageVectorStore(str(tmp_path / "vectors"), dim=4)
    embedder = FakeEmbedder()
    monkeypatch.setattr("app.image_indexer.get_image_store", lambda *a, **k: store)
    monkeypatch.setattr("app.image_indexer.get_image_embedder", lambda: embedder)
    monkeypatch.setattr("app.image_indexer.interaction.is_busy", lambda: False)

    indexer = ImageIndexer()
    indexer.start()
    for _ in range(100):
        status = indexer.status()
        if not status["running"] and status["finished_at"]:
            break
        time.sleep(0.05)

    status = indexer.status()
    assert status["done"] == 2, status
    assert status["failed"] == 0
    assert len(store.indexed()) == 2
    assert all("notes.txt" not in p for p in store.indexed())


def test_a_picture_that_is_already_indexed_is_skipped(pictures, tmp_path, monkeypatch):
    from app.image_store import file_hash

    store = ImageVectorStore(str(tmp_path / "vectors"), dim=4)
    picture = pictures / "photos" / "one.jpg"
    store.add([1.0, 0.0, 0.0, 0.0], "photos/one.jpg",
              content_hash=file_hash(picture))

    embedder = FakeEmbedder()
    monkeypatch.setattr("app.image_indexer.get_image_store", lambda *a, **k: store)
    monkeypatch.setattr("app.image_indexer.get_image_embedder", lambda: embedder)
    monkeypatch.setattr("app.image_indexer.interaction.is_busy", lambda: False)

    indexer = ImageIndexer()
    indexer.start()
    for _ in range(100):
        status = indexer.status()
        if not status["running"] and status["finished_at"]:
            break
        time.sleep(0.05)

    status = indexer.status()
    assert status["skipped"] == 1
    assert len(embedder.encoded) == 1  # only the other picture was encoded


def test_the_build_waits_while_someone_is_talking(pictures, tmp_path, monkeypatch):
    store = ImageVectorStore(str(tmp_path / "vectors"), dim=4)
    monkeypatch.setattr("app.image_indexer.get_image_store", lambda *a, **k: store)
    monkeypatch.setattr("app.image_indexer.get_image_embedder", lambda: FakeEmbedder())
    monkeypatch.setattr("app.image_indexer.interaction.is_busy", lambda: True)

    indexer = ImageIndexer()
    indexer.start()
    time.sleep(0.3)
    status = indexer.status()
    assert status["done"] == 0
    assert status["running"] is True
    indexer.stop()


def test_a_pause_waits_and_resume_finishes_the_job(pictures, tmp_path, monkeypatch):
    """A pause holds the picture it is on; it does not skip ahead."""
    store = ImageVectorStore(str(tmp_path / "vectors"), dim=4)
    monkeypatch.setattr("app.image_indexer.get_image_store", lambda *a, **k: store)
    monkeypatch.setattr("app.image_indexer.get_image_embedder", lambda: FakeEmbedder())
    monkeypatch.setattr("app.image_indexer.interaction.is_busy", lambda: False)

    indexer = ImageIndexer()
    indexer.start()
    indexer.pause()
    time.sleep(0.4)
    paused = indexer.status()
    assert paused["paused"] is True
    assert paused["running"] is True, "a pause waits, it does not end the run"
    encoded_while_paused = paused["encoded"]
    time.sleep(0.4)
    assert indexer.status()["encoded"] == encoded_while_paused, \
        "a paused build must not keep encoding"

    indexer.resume()
    for _ in range(200):
        status = indexer.status()
        if not status["running"] and status["finished_at"]:
            break
        time.sleep(0.05)
    assert indexer.status()["encoded"] == 2, "resume finishes the rest"


def test_a_busy_machine_waits_instead_of_skipping(pictures, tmp_path, monkeypatch):
    """Somebody talking holds the pictures up; none of them is lost."""
    store = ImageVectorStore(str(tmp_path / "vectors"), dim=4)
    monkeypatch.setattr("app.image_indexer.get_image_store", lambda *a, **k: store)
    monkeypatch.setattr("app.image_indexer.get_image_embedder", lambda: FakeEmbedder())
    busy = {"now": True}
    monkeypatch.setattr("app.image_indexer.interaction.is_busy", lambda: busy["now"])

    indexer = ImageIndexer()
    indexer.start()
    time.sleep(0.5)
    assert indexer.status()["encoded"] == 0, "nothing is encoded while someone talks"

    busy["now"] = False
    for _ in range(200):
        status = indexer.status()
        if not status["running"] and status["finished_at"]:
            break
        time.sleep(0.05)
    assert indexer.status()["encoded"] == 2, "the pictures were waited for, not dropped"


def test_one_bad_picture_does_not_stop_the_build(pictures, tmp_path, monkeypatch):
    """A picture that cannot be encoded is counted, and the run goes on."""
    store = ImageVectorStore(str(tmp_path / "vectors"), dim=4)

    class Picky:
        def embed_image(self, path):
            if "one.jpg" in str(path):
                raise RuntimeError("这张图坏了")
            return [1.0, 0.0, 0.0, 0.0]

    monkeypatch.setattr("app.image_indexer.get_image_store", lambda *a, **k: store)
    monkeypatch.setattr("app.image_indexer.get_image_embedder", lambda: Picky())
    monkeypatch.setattr("app.image_indexer.interaction.is_busy", lambda: False)

    indexer = ImageIndexer()
    indexer.start()
    for _ in range(100):
        status = indexer.status()
        if not status["running"] and status["finished_at"]:
            break
        time.sleep(0.05)

    status = indexer.status()
    assert status["failed"] == 1
    assert status["encoded"] == 1, status
    assert status["running"] is False


def test_a_batch_that_cannot_be_written_is_not_counted_as_done(pictures, tmp_path, monkeypatch):
    """The status must not claim pictures that never reached the table."""
    store = ImageVectorStore(str(tmp_path / "vectors"), dim=4)

    class BrokenStore:
        def add_many(self, vectors, records):
            raise RuntimeError("CommitConflict: table version conflict")

        def indexed(self):
            return {}

        def cleanup_old_versions(self, **kw):
            return {}

        def delete_missing(self, known):
            return 0

    monkeypatch.setattr("app.image_indexer.get_image_store", lambda *a, **k: BrokenStore())
    monkeypatch.setattr("app.image_indexer.get_image_embedder", lambda: FakeEmbedder())
    monkeypatch.setattr("app.image_indexer.interaction.is_busy", lambda: False)

    indexer = ImageIndexer()
    indexer.start()
    for _ in range(200):
        status = indexer.status()
        if not status["running"] and status["finished_at"]:
            break
        time.sleep(0.05)

    status = indexer.status()
    assert status["done"] == 0, "nothing was written, so nothing is done"
    assert status["failed"] > 0
    assert "写库失败" in status["last_error"], status["last_error"]


def test_an_incomplete_scan_does_not_empty_the_index(pictures, tmp_path, monkeypatch):
    """A directory that cannot be read looks like "the pictures are gone".

    Acting on that would empty the index, so an incomplete scan leaves
    it alone.
    """
    store = ImageVectorStore(str(tmp_path / "vectors"), dim=4)
    store.add([1.0, 0.0, 0.0, 0.0], "photos/one.jpg")

    monkeypatch.setattr("app.image_indexer.get_image_store", lambda *a, **k: store)
    monkeypatch.setattr("app.image_indexer.get_image_embedder", lambda: FakeEmbedder())
    monkeypatch.setattr("app.image_indexer.interaction.is_busy", lambda: False)
    # The scan sees nothing, but there are directories under the root.
    monkeypatch.setattr(ImageIndexer, "_pictures", lambda self, root: ([], False))

    indexer = ImageIndexer()
    indexer.start()
    for _ in range(100):
        status = indexer.status()
        if not status["running"] and status["finished_at"]:
            break
        time.sleep(0.05)

    assert store.count() == 1, "the index must survive an incomplete scan"
    assert "扫描没有完成" in indexer.status()["last_error"]


def test_a_missing_storage_root_leaves_the_index_alone(pictures, tmp_path, monkeypatch):
    """The storage root points at a directory that is not there."""
    store = ImageVectorStore(str(tmp_path / "vectors"), dim=4)
    store.add([1.0, 0.0, 0.0, 0.0], "photos/one.jpg")

    monkeypatch.setattr("app.image_indexer.get_image_store", lambda *a, **k: store)
    monkeypatch.setattr("app.image_indexer.get_image_embedder", lambda: FakeEmbedder())
    monkeypatch.setattr("app.image_indexer.interaction.is_busy", lambda: False)
    monkeypatch.setattr("app.deps.get_storage_root", lambda: tmp_path / "not-there")

    indexer = ImageIndexer()
    indexer.start()
    for _ in range(100):
        status = indexer.status()
        if not status["running"] and status["finished_at"]:
            break
        time.sleep(0.05)

    assert store.count() == 1
