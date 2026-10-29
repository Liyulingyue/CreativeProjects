"""Tests for chunked uploads and the conflict rules.

The rule that matters here is that an upload never replaces a file
without being asked to. Everything else -- the pieces, the order they
come back in, the size check -- exists so that the file that arrives
is the file that was sent.
"""

import pytest


@pytest.fixture
def manager(tmp_path):
    from app.uploads import UploadManager

    root = tmp_path / "storage"
    root.mkdir()
    return UploadManager(root), root


def _upload(manager, root, name="a.bin", payload=b"", chunks=1):
    svc, _ = manager
    meta = svc.init(root, name, len(payload), chunks)
    size = max(1, len(payload) // chunks) if payload else 0
    for i in range(chunks):
        piece = payload[i * size:(i + 1) * size] if payload else b""
        svc.write_chunk(meta["upload_id"], i, piece)
    return meta


def test_init_reports_nothing_when_the_name_is_free(manager):
    svc, root = manager
    meta = svc.init(root, "a.bin", 10, 1)
    assert meta["existing"] is None
    assert meta["total_chunks"] == 1


def test_init_reports_what_is_already_there(manager):
    svc, root = manager
    (root / "a.bin").write_bytes(b"old")
    meta = svc.init(root, "a.bin", 10, 1)
    assert meta["existing"]["size"] == 3
    assert meta["existing"]["name"] == "a.bin"


def test_pieces_can_arrive_out_of_order(manager):
    svc, root = manager
    meta = svc.init(root, "a.bin", 9, 3)
    # Piece 0 is "aaa", piece 1 is "bbb", piece 2 is "ccc" -- but they
    # arrive in the order 2, 0, 1, because that is how a network
    # chooses to deliver them.
    for i, piece in ((2, b"ccc"), (0, b"aaa"), (1, b"bbb")):
        svc.write_chunk(meta["upload_id"], i, piece)

    assert svc.received(meta["upload_id"]) == [0, 1, 2]
    svc.complete(meta["upload_id"])
    assert (root / "a.bin").read_bytes() == b"aaabbbccc"


def test_a_missing_piece_stops_the_finish(manager):
    svc, root = manager
    meta = svc.init(root, "a.bin", 6, 2)
    svc.write_chunk(meta["upload_id"], 0, b"aaa")

    from app.uploads import UploadError

    with pytest.raises(UploadError):
        svc.complete(meta["upload_id"])
    # Nothing was written to the destination.
    assert not (root / "a.bin").exists()


def test_a_file_of_the_wrong_size_is_refused(manager):
    svc, root = manager
    meta = svc.init(root, "a.bin", 10, 1)
    svc.write_chunk(meta["upload_id"], 0, b"short")

    from app.uploads import UploadError

    with pytest.raises(UploadError):
        svc.complete(meta["upload_id"])
    assert not (root / "a.bin").exists()


def test_an_existing_file_is_not_replaced_without_asking(manager):
    svc, root = manager
    (root / "a.bin").write_bytes(b"original")
    meta = svc.init(root, "a.bin", 3, 1)
    svc.write_chunk(meta["upload_id"], 0, b"new")

    from app.uploads import UploadConflict

    with pytest.raises(UploadConflict) as exc:
        svc.complete(meta["upload_id"])
    assert exc.value.existing["size"] == len(b"original")
    assert (root / "a.bin").read_bytes() == b"original"


def test_replace_puts_the_new_file_in_place(manager):
    svc, root = manager
    (root / "a.bin").write_bytes(b"original")
    meta = svc.init(root, "a.bin", 3, 1)
    svc.write_chunk(meta["upload_id"], 0, b"new")

    result = svc.complete(meta["upload_id"], overwrite="replace")
    assert result["replaced"] is True
    assert (root / "a.bin").read_bytes() == b"new"


def test_rename_writes_beside_the_original(manager):
    svc, root = manager
    (root / "a.bin").write_bytes(b"original")
    meta = svc.init(root, "a.bin", 3, 1)
    svc.write_chunk(meta["upload_id"], 0, b"new")

    result = svc.complete(meta["upload_id"], overwrite="rename")
    assert result["name"] == "a(1).bin"
    assert (root / "a.bin").read_bytes() == b"original"
    assert (root / "a(1).bin").read_bytes() == b"new"


def test_the_staging_area_is_emptied_when_the_upload_finishes(manager):
    svc, root = manager
    meta = svc.init(root, "a.bin", 3, 1)
    svc.write_chunk(meta["upload_id"], 0, b"abc")
    svc.complete(meta["upload_id"])

    staging = root / ".uploads"
    assert staging.exists()
    assert list(staging.iterdir()) == []


def test_abandoning_throws_the_pieces_away(manager):
    svc, root = manager
    meta = svc.init(root, "a.bin", 3, 1)
    svc.write_chunk(meta["upload_id"], 0, b"abc")

    svc.abandon(meta["upload_id"])

    assert not (root / ".uploads" / meta["upload_id"]).exists()


def test_the_staging_area_is_hidden_from_a_listing(manager):
    """The staging area lives under the storage root, so it has to be
    a dot-directory: the browser skips those."""
    svc, root = manager
    meta = svc.init(root, "a.bin", 3, 1)
    svc.write_chunk(meta["upload_id"], 0, b"abc")

    assert (root / ".uploads").name.startswith(".")
    visible = [p for p in root.iterdir() if not p.name.startswith(".")]
    assert visible == []


def test_an_upload_id_from_outside_is_refused(manager):
    svc, root = manager
    from app.uploads import UploadError

    for bad in ("../escape", "not-hex", "", "a" * 40):
        with pytest.raises(UploadError):
            svc.write_chunk(bad, 0, b"x")


def test_a_piece_out_of_range_is_refused(manager):
    svc, root = manager
    meta = svc.init(root, "a.bin", 3, 2)

    from app.uploads import UploadError

    with pytest.raises(UploadError):
        svc.write_chunk(meta["upload_id"], 5, b"x")
    with pytest.raises(UploadError):
        svc.write_chunk(meta["upload_id"], -1, b"x")


def test_a_dropped_folder_keeps_its_shape(tmp_path):
    """A folder dropped on the browser arrives as files below it."""
    from app.uploads import UploadManager

    manager = UploadManager(str(tmp_path / "staging"))
    target = tmp_path / "target"
    target.mkdir()

    info = manager.init(target, "a.txt", 4, 1, rel_dir="照片/2024")

    assert info["rel_dir"] == "照片/2024"
    assert (target / "照片" / "2024").is_dir()


def test_a_dropped_folder_cannot_climb_out(tmp_path):
    from app.uploads import UploadError, UploadManager

    manager = UploadManager(str(tmp_path / "staging"))
    target = tmp_path / "target"
    target.mkdir()

    with pytest.raises(UploadError):
        manager.init(target, "a.txt", 4, 1, rel_dir="../../etc")


def test_the_name_in_a_dropped_folder_is_just_the_name(tmp_path):
    from app.uploads import UploadManager

    manager = UploadManager(str(tmp_path / "staging"))
    target = tmp_path / "target"
    target.mkdir()

    info = manager.init(target, "a.txt", 4, 1, rel_dir="sub")
    assert info["name"] == "a.txt"
    assert info["target_dir"].endswith("/target/sub")
