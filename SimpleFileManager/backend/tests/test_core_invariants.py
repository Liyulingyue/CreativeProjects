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
