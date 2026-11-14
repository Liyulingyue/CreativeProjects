"""Tests for the picture vector table."""

from app.image_store import ImageVectorStore, file_hash


def _store(tmp_path, dim=4):
    return ImageVectorStore(str(tmp_path / "vectors"), dim=dim)


def test_a_picture_is_stored_and_found(tmp_path):
    store = _store(tmp_path)
    store.add([1.0, 0.0, 0.0, 0.0], "/root/apple.jpg", content_hash="h1")
    store.add([0.0, 1.0, 0.0, 0.0], "/root/cat.jpg", content_hash="h2")

    hits = store.search([1.0, 0.0, 0.0, 0.0], top_k=2)
    assert hits[0]["file_path"] == "/root/apple.jpg"
    assert hits[0]["score"] > hits[1]["score"]


def test_re_encoding_replaces_the_row(tmp_path):
    store = _store(tmp_path)
    store.add([1.0, 0.0, 0.0, 0.0], "/root/apple.jpg", content_hash="h1")
    store.add([0.0, 1.0, 0.0, 0.0], "/root/apple.jpg", content_hash="h2")

    assert store.count() == 1
    assert store.indexed()["/root/apple.jpg"] == "h2"


def test_needs_encoding_follows_the_content_hash(tmp_path):
    store = _store(tmp_path)
    store.add([1.0, 0.0, 0.0, 0.0], "/root/apple.jpg", content_hash="h1")
    assert store.needs_encoding("/root/apple.jpg", "h1") is False
    assert store.needs_encoding("/root/apple.jpg", "h2") is True
    assert store.needs_encoding("/root/new.jpg", "") is True


def test_deleting_a_picture(tmp_path):
    store = _store(tmp_path)
    store.add([1.0, 0.0, 0.0, 0.0], "/root/apple.jpg")
    store.delete_by_file("/root/apple.jpg")
    assert store.count() == 0


def test_rows_for_files_that_are_gone_are_dropped(tmp_path):
    store = _store(tmp_path)
    store.add([1.0, 0.0, 0.0, 0.0], "/root/apple.jpg")
    store.add([0.0, 1.0, 0.0, 0.0], "/root/gone.jpg")

    removed = store.delete_missing({"/root/apple.jpg"})
    assert removed == 1
    assert list(store.indexed()) == ["/root/apple.jpg"]


def test_search_on_an_empty_table_is_empty(tmp_path):
    store = _store(tmp_path)
    assert store.search([1.0, 0.0, 0.0, 0.0]) == []


def test_the_table_is_created_with_the_requested_width(tmp_path):
    store = _store(tmp_path, dim=8)
    schema = store._table.schema
    field = schema.field("vector")
    assert field.type.list_size == 8


def test_file_hash_follows_the_bytes(tmp_path):
    p = tmp_path / "a.bin"
    p.write_bytes(b"hello")
    first = file_hash(p)
    assert first == file_hash(p)
    p.write_bytes(b"hello!")
    assert file_hash(p) != first
