"""The rules that keep the big cores from being taken twice or not at all.

Every one of these was a hole in the test suite: the code was right, but
nothing watched it. A refactor could have broken any of them silently.
"""
import threading
import time

import pytest

from app.cpu_pool import CpuPoolBusy, get_big_pool
from app.interaction import interaction


def test_a_crash_inside_the_hold_does_not_lock_the_machine_up():
    """The worst failure of all: the pool is never given back.

    If an exception leaks the lock, every later request waits out its
    deadline and fails, for ever, with no way for the user to tell why.
    """
    pool = get_big_pool()
    with pytest.raises(RuntimeError):
        with pool.hold(timeout=5):
            raise RuntimeError("something went wrong half way through")
    # The pool must be free again straight away.
    with pool.hold(timeout=1):
        pass


def test_a_crash_inside_the_try_hold_does_not_lock_the_machine_up():
    pool = get_big_pool()
    with pytest.raises(ValueError):
        with pool.try_hold(timeout=1) as got:
            assert got is True
            raise ValueError("boom")
    with pool.hold(timeout=1):
        pass


def test_two_threads_cannot_hold_the_pool_at_once():
    """The model and the picture search must never share the cores."""
    pool = get_big_pool()
    order = []

    def first():
        with pool.hold(timeout=5):
            order.append("first in")
            time.sleep(0.3)
            order.append("first out")

    def second():
        time.sleep(0.05)
        with pool.hold(timeout=5):
            order.append("second in")
            order.append("second out")

    threads = [threading.Thread(target=first), threading.Thread(target=second)]
    for t in threads:
        t.start()
    for t in threads:
        t.join(timeout=10)

    assert order == ["first in", "first out", "second in", "second out"], order


def test_the_search_marks_the_machine_as_in_use(tmp_path, monkeypatch):
    """A search is an interaction: the index must stand down for it.

    The text indexer uses the same tracker. If a search did not mark
    itself, a background index build would keep the cores and the search
    would be told the machine is busy.
    """
    from app import image_search
    from app.image_store import ImageVectorStore

    store = ImageVectorStore(str(tmp_path / "vectors"), dim=4)
    store.add([1.0, 0.0, 0.0, 0.0], "photos/cat.jpg")
    monkeypatch.setattr("app.image_search._image_store", lambda: store)

    class FakeEmbedder:
        def embed_text(self, text, prefix=""):
            return [1.0, 0.0, 0.0, 0.0]

    monkeypatch.setattr("app.image_search.get_image_embedder", lambda: FakeEmbedder())

    marked = []
    original = interaction.mark_active

    def spy():
        marked.append(True)
        original()

    monkeypatch.setattr(interaction, "mark_active", spy)
    image_search.search_by_text("一只猫")
    assert marked, "the search did not tell the tracker anybody is home"


def test_the_score_is_the_cosine_similarity_and_not_the_distance(tmp_path):
    """The number next to a picture has to mean what it says.

    LanceDB hands back the *squared* distance, so the similarity is
    1 - distance/2. Leaving the half out would still rank the pictures
    in the right order (it is a monotone change), but every score would
    be wrong -- a picture that matches perfectly would read as -1.
    """
    from app.image_store import ImageVectorStore

    store = ImageVectorStore(str(tmp_path / "vectors"), dim=3)
    store.add([1.0, 0.0, 0.0], "photos/same.jpg")
    store.add([0.0, 1.0, 0.0], "photos/orthogonal.jpg")
    store.add([-1.0, 0.0, 0.0], "photos/opposite.jpg")

    hits = {h["file_path"]: h["score"] for h in store.search([1.0, 0.0, 0.0], top_k=5)}
    assert hits["photos/same.jpg"] == pytest.approx(1.0, abs=1e-3)
    assert hits["photos/orthogonal.jpg"] == pytest.approx(0.0, abs=1e-3)
    assert hits["photos/opposite.jpg"] == pytest.approx(-1.0, abs=1e-3)


def test_a_picture_that_is_deleted_stops_being_offered(tmp_path):
    """The table must not keep pictures that are no longer on the disk.

    Otherwise the search page shows a picture that cannot be opened, and
    the thumbnails come back as broken images.
    """
    from app.image_store import ImageVectorStore

    store = ImageVectorStore(str(tmp_path / "vectors"), dim=3)
    store.add([1.0, 0.0, 0.0], "photos/kept.jpg")
    store.add([0.0, 1.0, 0.0], "photos/gone.jpg")

    removed = store.delete_missing({"photos/kept.jpg"})
    assert removed == 1
    assert store.count() == 1
    hits = store.search([0.0, 1.0, 0.0], top_k=5)
    assert [h["file_path"] for h in hits] == ["photos/kept.jpg"]


def test_a_table_that_cannot_be_written_to_stops_the_build(tmp_path, monkeypatch):
    """Three failures in a row is a broken table, not a hiccup.

    Without a way out, the build would carry on failing for every
    picture in the library, writing a log line for each.
    """
    from app.deps import get_storage_root
    from app.image_indexer import ImageIndexer

    root = get_storage_root()
    for i in range(8):
        p = root / f"photos/broken{i}.jpg"
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_bytes(b"\xff\xd8\xff\xd9")

    class BrokenStore:
        def count(self):
            return 0

        def indexed(self):
            return {}

        def add_many(self, vectors, records):
            raise OSError("磁盘满了")

        def delete_missing(self, known):
            return 0

        def cleanup_old_versions(self, older_than_days=0):
            return 0

    class FakeEmbedder:
        def embed_image(self, path):
            return [1.0, 0.0, 0.0, 0.0]

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
    assert status["failed"] > 0, status
    # The build must have given up rather than going through all eight.
    assert status["failed"] < 8, status


def test_a_remote_provider_does_not_take_the_cores_in_the_tool_calling_path(
        monkeypatch):
    """The tool-calling chat must not lock the cores for a cloud model.

    The agent talks to the model with tools, and that goes through the
    non-streaming call. A remote provider uses none of this machine's
    cores, so locking them would stop the local model for the length of
    a round trip to the other side of the world.
    """
    from app import llm_client

    class FakeCompletions:
        def create(self, **kwargs):
            message = type("M", (), {"content": "hello", "tool_calls": None})()
            choice = type("Ch", (), {"message": message})()
            return type("R", (), {"choices": [choice]})()

    class FakeClient:
        chat = type("Chat", (), {"completions": FakeCompletions()})()

    monkeypatch.setattr(llm_client, "_get_client", lambda provider: FakeClient())
    remote = type("P", (), {"model": "m", "max_tokens": None,
                            "reasoning_budget": None,
                            "base_url": "https://api.example.com/v1"})()

    llm_client.chat_completion([{"role": "user", "content": "hi"}], provider=remote)
    with get_big_pool().try_hold(timeout=0) as got:
        assert got is True, "a remote provider locked the local cores"


def test_the_quota_message_names_the_way_out():
    """402 means the cloud account is out of money -- say that, not "error".

    The demo runs against the local model, and the default provider is
    the cloud one; this message is what tells the operator to switch.
    """
    from openai import APIStatusError

    from app.llm_client import _friendly_error

    class FakeResponse:
        status_code = 402
        headers = {}
        text = "Payment Required"

        class _Req:
            method = "POST"
            url = "http://example"

        request = _Req()

    message = _friendly_error(
        APIStatusError("Payment required", response=FakeResponse(), body=None))
    assert "额度" in message, message
    assert "Model24B" in message, message


def test_the_tool_calling_chat_holds_the_cores_while_the_model_works(monkeypatch):
    """The agent's round trip is a non-streaming call -- and it counts.

    The streaming path was watched; this one was not. If the tool-calling
    request went out without the lock, the picture index would believe
    it owns the cores while the model is computing on them.
    """
    from app import llm_client

    busy_during_request = []

    class FakeCompletions:
        def create(self, **kwargs):
            # While the request is in flight, are the cores ours?
            with get_big_pool().try_hold(timeout=0) as got:
                busy_during_request.append(not got)
            message = type("M", (), {"content": "hello", "tool_calls": None})()
            choice = type("Ch", (), {"message": message})()
            return type("R", (), {"choices": [choice]})()

    class FakeClient:
        chat = type("Chat", (), {"completions": FakeCompletions()})()

    monkeypatch.setattr(llm_client, "_get_client", lambda provider: FakeClient())
    local = type("P", (), {"model": "m", "max_tokens": None,
                           "reasoning_budget": None,
                           "base_url": "http://127.0.0.1:9010/v1"})()

    llm_client.chat_completion([{"role": "user", "content": "hi"}], provider=local)
    assert busy_during_request == [True], "the model was not holding the cores"


def test_a_remote_tool_calling_chat_does_not_take_the_cores(monkeypatch):
    """The same call, but to the cloud: the cores must stay free.

    A cloud round trip can take thirty seconds. Holding the cores for it
    would stop the picture search for thirty seconds over a call that
    never used them.
    """
    from app import llm_client

    busy_during_request = []

    class FakeCompletions:
        def create(self, **kwargs):
            with get_big_pool().try_hold(timeout=0) as got:
                busy_during_request.append(not got)
            message = type("M", (), {"content": "hello", "tool_calls": None})()
            choice = type("Ch", (), {"message": message})()
            return type("R", (), {"choices": [choice]})()

    class FakeClient:
        chat = type("Chat", (), {"completions": FakeCompletions()})()

    monkeypatch.setattr(llm_client, "_get_client", lambda provider: FakeClient())
    remote = type("P", (), {"model": "m", "max_tokens": None,
                            "reasoning_budget": None,
                            "base_url": "https://api.example.com/v1"})()

    llm_client.chat_completion([{"role": "user", "content": "hi"}], provider=remote)
    assert busy_during_request == [False], "a remote provider locked the local cores"


def test_a_file_that_only_pretends_to_be_a_picture_is_not_encoded(tmp_path,
                                                                  monkeypatch):
    """A .jpg that is not a picture must not cost the big cores.

    A download that failed leaves an HTML page under a name that says
    picture. Encoding it takes three seconds of the cores and comes back
    as an error; the file has to be recognised by its first bytes.
    """
    from app.deps import get_storage_root
    from app.image_indexer import ImageIndexer

    root = get_storage_root()
    (root / "photos").mkdir(parents=True, exist_ok=True)
    (root / "photos" / "fake.jpg").write_text("<html>not found</html>")
    (root / "photos" / "empty.jpg").write_bytes(b"")
    (root / "photos" / "real.jpg").write_bytes(b"\xff\xd8\xff\xe0" + b"\x00" * 64)

    indexer = ImageIndexer()
    found, complete = indexer._pictures(root)
    names = {p.name for p in found}
    assert "real.jpg" in names
    assert "fake.jpg" not in names, "a file that is not a picture was queued for encoding"
    assert "empty.jpg" not in names
    assert indexer.status()["not_pictures"] >= 1


def test_the_status_keeps_its_shape_between_runs(tmp_path, monkeypatch):
    """Every field the page reads has to be there, always.

    The status used to be built in two places -- once at the start and
    once at the beginning of a run -- and a field added to one of them
    was missing from the other. The page then showed "undefined" for a
    count that was perfectly well known.
    """
    from app.deps import get_storage_root
    from app.image_indexer import ImageIndexer

    root = get_storage_root()
    (root / "photos").mkdir(parents=True, exist_ok=True)
    (root / "photos" / "one.jpg").write_bytes(b"\xff\xd8\xff\xe0" + b"\x00" * 32)

    indexer = ImageIndexer()
    before = set(indexer.status().keys())
    indexer.start()
    for _ in range(100):
        status = indexer.status()
        if not status["running"] and status["finished_at"]:
            break
        time.sleep(0.05)
    after = set(indexer.status().keys())
    assert before == after, after - before


def test_a_pause_does_not_outlive_the_build_it_was_meant_for(tmp_path,
                                                             monkeypatch):
    """Pausing a build that is not running must leave nothing behind.

    The page can be a few seconds out of date, so the user can click
    "pause" after the build has already finished. If that left the flag
    set, the *next* build would report itself as paused while it was
    working, and the page would say "已暂停" over a running build.
    """
    from app.deps import get_storage_root
    from app.image_indexer import ImageIndexer

    root = get_storage_root()
    (root / "photos").mkdir(parents=True, exist_ok=True)
    (root / "photos" / "one.jpg").write_bytes(b"\xff\xd8\xff\xe0" + b"\x00" * 32)

    class FakeEmbedder:
        def embed_image(self, path):
            return [1.0, 0.0, 0.0, 0.0]

    monkeypatch.setattr("app.image_indexer.get_image_embedder", lambda: FakeEmbedder())
    monkeypatch.setattr("app.image_indexer.interaction.is_busy", lambda: False)

    indexer = ImageIndexer()
    indexer.start()
    for _ in range(100):
        status = indexer.status()
        if not status["running"] and status["finished_at"]:
            break
        time.sleep(0.05)

    # The build is over; a pause now is a pause of nothing.
    result = indexer.pause()
    assert result.get("started") is False, result
    assert indexer.status()["paused"] is False

    # And the next build must not inherit the pause.
    indexer.start(rebuild=True)
    time.sleep(0.2)
    assert indexer.status()["paused"] is False, indexer.status()


def test_a_picture_that_was_deleted_is_not_offered(tmp_path, monkeypatch):
    """Deleting a picture must take it out of the search at once.

    The index is built once a day, so a picture deleted in the morning
    would still be handed out in the evening -- with its name, its size
    and a thumbnail that cannot draw. The search drops it, and drops
    the row with it, so the table does not fill up with ghosts.
    """
    from app.deps import get_storage_root
    from app.image_search import search_by_text
    from app.image_store import ImageVectorStore

    root = get_storage_root()
    picture = root / "photos" / "gone.jpg"
    picture.parent.mkdir(parents=True, exist_ok=True)
    picture.write_bytes(b"\xff\xd8\xff\xe0" + b"\x00" * 32)

    store = ImageVectorStore(str(tmp_path / "vectors"), dim=4)
    store.add([1.0, 0.0, 0.0, 0.0], "photos/gone.jpg")
    store.add([0.0, 1.0, 0.0, 0.0], "photos/kept.jpg")
    monkeypatch.setattr("app.image_search._image_store", lambda: store)

    class FakeEmbedder:
        def embed_text(self, text, prefix=""):
            return [1.0, 0.0, 0.0, 0.0]

    monkeypatch.setattr("app.image_search.get_image_embedder", lambda: FakeEmbedder())

    # The picture is there to begin with.
    assert [r["path"] for r in search_by_text("一张图")] == ["photos/gone.jpg"]

    picture.unlink()
    assert search_by_text("一张图") == []
    # And the row is gone too, so the table does not keep the ghost.
    assert "photos/gone.jpg" not in store.indexed()


def test_a_renamed_picture_keeps_its_vector(tmp_path, monkeypatch):
    """Moving a picture does not change what is in it.

    Re-encoding it would cost three seconds of the big cores -- the ones
    the model wants -- and the score would not change either.
    """
    from app.image_store import ImageVectorStore

    store = ImageVectorStore(str(tmp_path / "vectors"), dim=3)
    store.add([1.0, 0.0, 0.0], "photos/before.jpg")

    assert store.move("photos/before.jpg", "photos/after.jpg") is True
    assert "photos/before.jpg" not in store.indexed()
    assert "photos/after.jpg" in store.indexed()

    hits = store.search([1.0, 0.0, 0.0], top_k=3)
    assert [h["file_path"] for h in hits] == ["photos/after.jpg"]
    assert hits[0]["score"] == pytest.approx(1.0, abs=1e-3)


def test_a_picture_in_the_bin_is_not_offered_and_leaves_no_ghost(
        tmp_path, monkeypatch):
    """A picture that was put in the bin must not turn up in a search.

    The path it had is gone, so the search skips it -- and the row goes
    with it, so the table does not fill up with names of files that are
    not there. (Putting the picture back makes it findable again at the
    next index window, not before.)
    """
    from app.deps import get_storage_root
    from app.image_search import search_by_text
    from app.image_store import ImageVectorStore

    root = get_storage_root()
    picture = root / "photos" / "binned.jpg"
    picture.parent.mkdir(parents=True, exist_ok=True)
    picture.write_bytes(b"\xff\xd8\xff\xe0" + b"\x00" * 32)

    store = ImageVectorStore(str(tmp_path / "vectors"), dim=4)
    store.add([1.0, 0.0, 0.0, 0.0], "photos/binned.jpg")
    monkeypatch.setattr("app.image_search._image_store", lambda: store)

    class FakeEmbedder:
        def embed_text(self, text, prefix=""):
            return [1.0, 0.0, 0.0, 0.0]

    monkeypatch.setattr("app.image_search.get_image_embedder", lambda: FakeEmbedder())

    assert [r["path"] for r in search_by_text("一张图")] == ["photos/binned.jpg"]

    # Into the bin: the file is not where it was.
    picture.unlink()
    assert search_by_text("一张图") == []
    assert "photos/binned.jpg" not in store.indexed()


def test_two_conversations_may_generate_at_the_same_time(monkeypatch):
    """The lock must not be stricter than the server.

    The llama-server here was started with --parallel 2: it serves two
    conversations at once and queues the rest itself. With the old
    mutex the second conversation waited for the first to finish, and
    then failed after a minute.
    """
    from app import llm_client
    from app.cpu_pool import get_big_pool

    inside = []
    both_inside = threading.Event()

    class FakeCompletions:
        def create(self, **kwargs):
            inside.append(1)
            if len(inside) >= 2:
                both_inside.set()
            # Both requests have to be in flight at the same time.
            both_inside.wait(5)
            message = type("M", (), {"content": "hi", "tool_calls": None})()
            choice = type("Ch", (), {"message": message})()
            return type("R", (), {"choices": [choice]})()

    class FakeClient:
        chat = type("Chat", (), {"completions": FakeCompletions()})()

    monkeypatch.setattr(llm_client, "_get_client", lambda provider: FakeClient())
    local = type("P", (), {"model": "m", "max_tokens": None,
                           "reasoning_budget": None,
                           "base_url": "http://127.0.0.1:9010/v1"})()

    results = []

    def talk():
        try:
            llm_client.chat_completion([{"role": "user", "content": "hi"}],
                                       provider=local)
            results.append("ok")
        except Exception as e:  # noqa: BLE001
            results.append(f"{type(e).__name__}: {e}")

    threads = [threading.Thread(target=talk) for _ in range(2)]
    for t in threads:
        t.start()
    for t in threads:
        t.join(20)

    assert results == ["ok", "ok"], results
    assert len(inside) == 2, inside


def _store_with_one_picture(tmp_path, monkeypatch):
    """A picture table with one picture in it, really on disk."""
    from app.deps import get_storage_root
    from app.image_store import ImageVectorStore

    root = get_storage_root()
    picture = root / "photos" / "one.jpg"
    picture.parent.mkdir(parents=True, exist_ok=True)
    picture.write_bytes(b"\xff\xd8\xff\xe0" + b"\x00" * 32)
    store = ImageVectorStore(str(tmp_path / "vectors"), dim=4)
    store.add([1.0, 0.0, 0.0, 0.0], "photos/one.jpg")
    monkeypatch.setattr("app.image_search._image_store", lambda: store)
    return store


def test_a_client_that_stops_reading_gives_the_cores_back(monkeypatch):
    """A stalled client must not hold the box.

    The browser tab went to the background, or the phone fell asleep: the
    server stops generating, so the cores are idle -- but the lease says
    the model is using them, and nothing else on the machine can get
    them. The lease notices that the client took too long to take a
    chunk and hands them back.
    """
    from app import llm_client
    from app.cpu_pool import get_big_pool

    monkeypatch.setattr(llm_client, "SLOW_CLIENT_SECONDS", 0.3)

    def fake_chunk():
        delta = type("D", (), {"content": "hello", "reasoning_content": None,
                               "tool_calls": None})()
        choice = type("C", (), {"delta": delta})()
        return type("Chunk", (), {"choices": [choice]})()

    class FakeStream:
        def __iter__(self):
            return iter([fake_chunk(), fake_chunk(), fake_chunk()])

    class FakeCompletions:
        def create(self, **kwargs):
            return FakeStream()

    class FakeClient:
        chat = type("Chat", (), {"completions": FakeCompletions()})()

    monkeypatch.setattr(llm_client, "_get_client", lambda provider: FakeClient())
    local = type("P", (), {"model": "m", "max_tokens": None,
                           "reasoning_budget": None,
                           "base_url": "http://127.0.0.1:9010/v1"})()

    stream = llm_client.chat_completion_stream(
        [{"role": "user", "content": "hi"}], provider=local)

    # Take the first chunk, then stop reading for longer than the
    # threshold: this is the browser tab in the background.
    next(iter(stream))
    time.sleep(0.9)

    # While the client is not reading, the cores must be free for
    # somebody else.
    with get_big_pool().try_hold(timeout=0) as got:
        assert got is True, "the stalled client still holds the cores"
    stream.close()


def test_a_text_search_runs_next_to_a_conversation(tmp_path, monkeypatch):
    """A text query is a fraction of a second: it need not wait.

    It takes a reader's lease, so it can run while the model is
    answering. A picture query cannot: it encodes the picture itself,
    seconds of every core, and that is a writer's lease.
    """
    from app import image_search
    from app.cpu_pool import get_big_pool

    class FakeEmbedder:
        def embed_text(self, text, prefix=""):
            return [1.0, 0.0, 0.0, 0.0]

        def embed_image(self, path):
            return [1.0, 0.0, 0.0, 0.0]

    monkeypatch.setattr("app.image_search.get_image_embedder", lambda: FakeEmbedder())

    store = _store_with_one_picture(tmp_path, monkeypatch)

    # The model is answering: it holds a reader's lease.
    pool = get_big_pool()
    with pool.hold(timeout=5, shared=True):
        results = image_search.search_by_text("一张图")
        assert [r["path"] for r in results] == ["photos/one.jpg"]


def test_the_thumbnail_cache_does_not_grow_for_ever(tmp_path):
    """A picture that was replaced leaves its old little picture behind.

    The name of a cached picture carries the size and the moment the file
    was changed, so every version of a photograph is a file of its own.
    Nothing else ever removed them, and on a machine with many
    photographs the cache would grow until the disk was full.
    """
    from app.thumbnails import ThumbnailStore

    store = ThumbnailStore(tmp_path, tmp_path / "thumbs")
    store.cache_root.mkdir(parents=True, exist_ok=True)
    for i in range(30):
        old = time.time() - (100 - i)
        path = store.cache_root / f"pic{i}.jpg"
        path.write_bytes(b"\xff\xd8\xff")
        import os
        os.utime(path, (old, old))

    removed = store.prune(limit=10)
    assert removed == 20, removed
    left = list(store.cache_root.iterdir())
    assert len(left) == 10
    # The oldest are the ones that went.
    assert "pic29.jpg" in {p.name for p in left}
    assert "pic0.jpg" not in {p.name for p in left}


def test_the_text_table_opens_again_after_a_restore(tmp_path):
    """A handle to a table whose directory was replaced is a handle to
    nothing.

    Putting a backup back, or moving the data directory underneath a
    running server, replaces the directory. The old handle keeps
    reading the old rows and refuses to write, and the search answers
    with files that are not there any more.
    """
    import shutil

    from app.deps import LanceDBVectorStore

    db_path = tmp_path / "vectors"
    store = LanceDBVectorStore(str(db_path), dim=4)
    store.add([1.0, 0.0, 0.0, 0.0], {"file_path": "a.txt", "content": "a",
                                      "chunk_idx": 0})
    assert store.count() == 1

    # The directory is replaced, the way a restore does it.
    moved = tmp_path / "vectors-old"
    shutil.move(str(db_path), str(moved))
    fresh = LanceDBVectorStore(str(db_path), dim=4)
    fresh.add([0.0, 1.0, 0.0, 0.0], {"file_path": "b.txt", "content": "b",
                                      "chunk_idx": 0})

    # The old handle must notice and open the new table.
    assert store.count() == 1
    rows = store.table.to_arrow().select(["file_path"]).to_pylist()
    assert [r["file_path"] for r in rows] == ["b.txt"], rows


def test_the_conversations_are_listed_without_holding_the_write_lock(tmp_path):
    """A page that lists the conversations must not stand in front of a
    write.

    The lock used to cover the reads as well, and the list reads every
    message of every conversation -- so a page load held the lock for as
    long as it took, and the agent's answer, which has to be written
    down before the turn can end, waited behind it. One slow list of
    conversations could stop the whole service from talking.
    """
    from app.deps import ChatHistoryService

    service = ChatHistoryService(str(tmp_path / "chat.db"))
    service.create_session("t")
    service.add_message(service.get_sessions()[0].id, role="user", content="hello")

    # A reader is at work.
    reading = threading.Event()
    done = threading.Event()

    def reader():
        reading.set()
        service.get_sessions()
        done.set()

    t = threading.Thread(target=reader)
    t.start()
    reading.wait(2)
    # The write must not wait for the reader.
    started = time.time()
    service.add_message(service.get_sessions()[0].id, role="assistant", content="hi")
    assert time.time() - started < 2.0, "the write waited for a reader"
    t.join(5)
    assert done.is_set()


def test_a_broken_row_does_not_break_the_whole_list(tmp_path):
    """One conversation with a malformed column must not take the rest
    of the list with it."""
    import sqlite3

    from app.deps import ChatHistoryService

    service = ChatHistoryService(str(tmp_path / "chat.db"))
    session = service.create_session("t")
    service.add_message(session.id, role="user", content="hello")

    # A row with a blob that is not JSON.
    conn = sqlite3.connect(str(tmp_path / "chat.db"))
    conn.execute("UPDATE chat_messages SET sources = 'not json at all'")
    conn.commit()
    conn.close()

    sessions = service.get_sessions()
    assert len(sessions) == 1
    assert sessions[0].messages[0].sources is None


def test_a_read_does_not_stand_in_front_of_a_write(tmp_path):
    """Every store that has a lock used to take it for reading too.

    The database is opened in WAL mode -- a reader works while a writer
    works -- so the reads do not need the lock, and taking it meant
    that one slow list of conversations could stop the service talking.
    The user table is the worst case: every request checks a token
    through it.
    """
    from app.users import UserStore

    store = UserStore(str(tmp_path / "users.db"))
    store.create_user("someone", "password123", role="user")
    store.store_token("tok", "someone", 0)

    # A read is at work.
    reading = threading.Event()
    finished = threading.Event()

    def reader():
        reading.set()
        for _ in range(50):
            store.get_by_username("someone")
        finished.set()

    t = threading.Thread(target=reader)
    t.start()
    reading.wait(2)

    # The write must not wait for the reader.
    started = time.time()
    store.set_password(store.list_users()[0]["id"], "another-password")
    assert time.time() - started < 2.0, "the write waited for a reader"
    t.join(5)
    assert finished.is_set()


def test_a_password_is_not_hashed_inside_the_lock(tmp_path):
    """Hashing a password is a hundred thousand rounds of sha256.

    It was done while the user table's lock was held, and every request
    that checks a token waits for that lock -- so creating a user made
    the whole service slower for as long as the hash took.
    """
    import app.users as users_mod

    store = users_mod.UserStore(str(tmp_path / "users.db"))
    user = store.create_user("someone", "password123", role="user")

    # The lock is taken for a moment; if the hash were inside it, the
    # lock would still be held while the hash runs.
    held = threading.Event()
    release = threading.Event()

    original = users_mod.hash_password

    def slow_hash(password, salt):
        held.set()
        release.wait(5)
        return original(password, salt)

    users_mod.hash_password = slow_hash
    try:
        t = threading.Thread(target=lambda: store.set_password(user["id"], "another-one"))
        t.start()
        assert held.wait(5), "the hash was never reached"
        # While the hash runs, a read must not be waiting for the lock.
        started = time.time()
        store.get_by_username("someone")
        assert time.time() - started < 1.0, "a read waited for the password hash"
        release.set()
        t.join(10)
    finally:
        users_mod.hash_password = original


def test_an_unplanned_error_answers_in_words(tmp_path):
    """A failure nobody planned for still says something.

    Without a handler the answer is FastAPI's plain "Internal Server
    Error", and the page shows a sentence with nothing in it. The
    traceback belongs in the log; the person in front of the screen
    gets a sentence they can act on.
    """
    from fastapi.testclient import TestClient

    from app.main import app

    @app.get("/api/_test_boom")
    def _boom():
        raise RuntimeError("something nobody planned for")

    client = TestClient(app, raise_server_exceptions=False)
    r = client.get("/api/_test_boom")
    assert r.status_code == 500
    detail = r.json().get("detail", "")
    assert detail, "没有给用户任何话"
    assert "Internal Server Error" not in detail
    assert "服务器" in detail or "稍后" in detail, detail


def test_a_busy_store_answers_with_a_sentence_and_not_a_crash(tmp_path):
    """The stores take a lock with a deadline.

    When the deadline passes the caller is told, in words it can show:
    the machine is busy, and trying again in a moment will work.
    """
    from fastapi.testclient import TestClient

    from app.main import app

    @app.get("/api/_test_busy")
    def _busy():
        raise TimeoutError("用户表忙，10 秒内没有拿到写锁")

    client = TestClient(app, raise_server_exceptions=False)
    r = client.get("/api/_test_busy")
    assert r.status_code == 503
    assert "忙" in r.json().get("detail", "")
