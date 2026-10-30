"""Tests for the little pictures of big ones.

They are made from the file itself, so a picture that only claims to
be one does not get a picture. And the key that lets a browser ask
for them opens the pictures of one folder and nothing else.
"""

import pytest


@pytest.fixture
def store(tmp_path):
    from app.thumbnails import ThumbnailStore

    root = tmp_path / "storage"
    root.mkdir()
    return ThumbnailStore(root, cache_root=tmp_path / "thumbs")


def _image(path, size=(800, 600), color=(200, 60, 60)):
    from PIL import Image

    img = Image.new("RGB", size, color)
    img.save(path)
    return path


def test_a_picture_gets_a_little_picture(store):
    pytest.importorskip("PIL")
    target = store.storage_root / "photo.jpg"
    _image(target)

    data = store.get(target, 256)

    assert data is not None
    from PIL import Image
    import io

    img = Image.open(io.BytesIO(data))
    assert max(img.size) == 256


def test_a_file_that_is_not_a_picture_does_not_get_one(store):
    target = store.storage_root / "notes.txt"
    target.write_text("just words")

    assert store.get(target, 256) is None


def test_a_picture_with_a_lie_in_its_name_does_not_get_one(store):
    pytest.importorskip("PIL")
    target = store.storage_root / "fake.jpg"
    target.write_text("this is not a picture at all")

    assert store.get(target, 256) is None


def test_the_little_picture_is_kept(store):
    pytest.importorskip("PIL")
    target = store.storage_root / "photo.jpg"
    _image(target)

    first = store.get(target, 256)
    second = store.get(target, 256)

    assert first == second
    assert len(list(store.cache_root.iterdir())) == 1


def test_a_replaced_file_gets_a_new_little_picture(store):
    pytest.importorskip("PIL")
    target = store.storage_root / "photo.jpg"
    _image(target, color=(200, 60, 60))
    first = store.get(target, 256)

    import time

    time.sleep(1.1)
    _image(target, color=(60, 200, 60))
    second = store.get(target, 256)

    assert first != second


def test_the_key_opens_the_pictures_of_one_folder(store):
    from app.routers.thumbnails import issue_directory_token, verify_directory_token

    directory = str(store.storage_root)
    issued = issue_directory_token(directory)

    assert verify_directory_token(str(store.storage_root / "photo.jpg"), issued["token"])
    assert not verify_directory_token(str(store.storage_root / "other" / "photo.jpg"), issued["token"])


def test_a_key_that_has_expired_opens_nothing(store):
    import time

    from app.routers.thumbnails import issue_directory_token, verify_directory_token

    directory = str(store.storage_root)
    issued = issue_directory_token(directory, ttl=-1)

    assert not verify_directory_token(str(store.storage_root / "photo.jpg"), issued["token"])
