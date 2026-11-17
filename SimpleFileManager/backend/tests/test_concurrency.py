"""Several people doing things at the same time.

The service has one lock for the big cores, one for each database, and
one for the storage directory. This file asks whether they hold up when
a chat, a picture search and a few uploads all happen at once -- the
kind of thing that happens on a NAS that three people share.

The bar is not "everything succeeds". It is: nothing deadlocks, nothing
raises something the caller cannot understand, and the store is left
in a state the next request can use.
"""

import threading
import time

import pytest

from conftest import client

from app.deps import state, get_storage_root


def _headers():
    """The headers a request needs.

    The test suite runs with the local-development escape hatch, where
    there is no login to do. When there is one, use it.
    """
    res = client.post("/api/auth/login",
                      json={"username": "admin", "password": "testpass"})
    if res.status_code == 200 and "token" in res.json():
        return {"Authorization": f"Bearer {res.json()['token']}"}
    return {}


class TestSeveralThingsAtOnce:
    def test_uploads_of_the_same_name_do_not_corrupt_each_other(self):
        """Two uploads of the same name, at the same time.

        One of them has to win and the other has to be told the name is
        taken. What must not happen is a half-written file: a file
        whose contents are one upload's first half and the other's
        second half.
        """
        headers = _headers()
        name = "同一时刻.txt"
        results = []
        barrier = threading.Barrier(4)

        def upload(i):
            barrier.wait()
            res = client.post(
                "/api/fs/upload?path=",
                files={"files": (name, f"content from {i}".encode(), "text/plain")},
                headers=headers,
            )
            results.append((i, res.status_code))

        threads = [threading.Thread(target=upload, args=(i,)) for i in range(4)]
        for t in threads:
            t.start()
        for t in threads:
            t.join(timeout=60)
        assert not any(t.is_alive() for t in threads), "an upload hung"

        ok = [i for i, code in results if code == 200]
        refused = [i for i, code in results if code == 409]
        assert len(ok) == 1, f"expected exactly one upload to win: {results}"
        assert len(refused) == 3, f"the others should be told the name is taken: {results}"

        target = get_storage_root() / name
        assert target.exists()
        body = target.read_text()
        assert body in {f"content from {i}" for i in ok}, \
            f"the file is a mix of two uploads: {body!r}"

    def test_reads_and_writes_at_the_same_time_do_not_deadlock(self):
        """A chat, a picture search and some file operations, together.

        Each of them takes a lock of its own. The point is that none of
        them waits for the others for ever.
        """
        headers = _headers()
        errors = []
        done = []

        def browse(i):
            try:
                res = client.get("/api/fs/browse?path=", headers=headers)
                if res.status_code != 200:
                    errors.append(f"browse {i}: {res.status_code}")
                else:
                    done.append("browse")
            except Exception as e:
                errors.append(f"browse {i}: {e}")

        def search(i):
            try:
                res = client.get("/api/fs/tree?depth=3", headers=headers)
                if res.status_code != 200:
                    errors.append(f"tree {i}: {res.status_code}")
                else:
                    done.append("tree")
            except Exception as e:
                errors.append(f"tree {i}: {e}")

        def upload(i):
            try:
                res = client.post(
                    "/api/fs/upload?path=",
                    files={"files": (f"并发-{i}.txt", f"body {i}".encode(), "text/plain")},
                    headers=headers,
                )
                if res.status_code != 200:
                    errors.append(f"upload {i}: {res.status_code}")
                else:
                    done.append("upload")
            except Exception as e:
                errors.append(f"upload {i}: {e}")

        threads = ([threading.Thread(target=browse, args=(i,)) for i in range(3)]
                   + [threading.Thread(target=search, args=(i,)) for i in range(3)]
                   + [threading.Thread(target=upload, args=(i,)) for i in range(3)])
        started = time.monotonic()
        for t in threads:
            t.start()
        for t in threads:
            t.join(timeout=90)
        assert not any(t.is_alive() for t in threads), "something hung"
        assert time.monotonic() - started < 90, "it took too long"
        assert not errors, errors
        assert len(done) == 9, done

        # And the service still works afterwards.
        res = client.get("/api/fs/browse?path=", headers=headers)
        assert res.status_code == 200
        for i in range(3):
            target = get_storage_root() / f"并发-{i}.txt"
            assert target.exists()
            target.unlink()

    def test_the_stores_are_left_usable(self):
        """After all that, every store still answers.

        A lock that was not given back, or a database left in a
        transaction, shows up here: the next request hangs or errors.
        """
        headers = _headers()
        for _ in range(3):
            assert client.get("/api/fs/browse?path=", headers=headers).status_code == 200
            assert client.get("/api/favorites", headers=headers).status_code == 200
            assert client.get("/api/shares", headers=headers).status_code == 200
            assert client.get("/api/uploads?limit=5", headers=headers).status_code == 200


class TestTwoChunkedUploadsOfTheSameName:
    def test_only_one_of_them_lands(self):
        """The same thing, but in pieces.

        The pieces are joined in a temporary file and then moved into
        place. If the temporary file's name is not unique, two uploads
        write over each other there -- and one of them puts the other's
        half-file at the destination.
        """
        root = get_storage_root()
        name = "分块同一时刻.txt"
        body = "body!"
        results = []
        barrier = threading.Barrier(4)

        def upload(i):
            barrier.wait()
            init = client.post("/api/fs/upload/init", params={
                "path": str(root), "name": name,
                "size": len(body), "total_chunks": 1,
            })
            upload_id = init.json()["upload_id"]
            client.post(f"/api/fs/upload/chunk?upload_id={upload_id}&index=0",
                        content=body.encode())
            comp = client.post("/api/fs/upload/complete", params={
                "upload_id": upload_id, "overwrite": "never",
            })
            results.append((i, comp.status_code))

        threads = [threading.Thread(target=upload, args=(i,)) for i in range(4)]
        for t in threads:
            t.start()
        for t in threads:
            t.join(timeout=60)
        assert not any(t.is_alive() for t in threads), "an upload hung"

        ok = [i for i, code in results if code == 200]
        refused = [i for i, code in results if code == 409]
        assert len(ok) == 1, f"expected exactly one upload to win: {results}"
        assert len(refused) == 3, f"the others should be told the name is taken: {results}"

        target = root / name
        assert target.read_text() == body
        leftovers = [p.name for p in root.iterdir() if p.name.endswith(".uploading")]
        assert leftovers == [], f"temporary files were left behind: {leftovers}"
        target.unlink()
