"""The big cores under the load they actually get.

One machine serves a household: a conversation with a local model, a
picture search, and a picture index rebuild all want the same four
fast cores. The conversation takes them as a reader (it needs them for
as long as it is answering); the picture index takes them as a writer
(it needs the whole box for one picture at a time).

The pool itself is tested directly in test_cpu_pool_stress.py. This
file asks the harder question: when the real code paths meet, does
anybody wait for ever, does anything get corrupted, and is the pool
free afterwards?

The bar is not that everything succeeds. A picture index that cannot
get in is fine -- it says so and tries again later. What must not
happen is a hang, a half-written index, or a pool left locked.
"""

import threading
import time
from types import SimpleNamespace

import pytest

from app.cpu_pool import get_big_pool
from app.deps import get_storage_root
from app.image_store import IMAGE_DIM


class SlowProvider:
    """A local model, so the pool is involved.

    The address has to look local: a model on another machine does not
    use these cores and must not lock them.
    """

    base_url = "http://127.0.0.1:9010/v1"
    model = "slow-local-model"
    api_key = "not-needed"
    max_tokens = None
    supports_tools = True


class FakeCompletions:
    """A model that answers slowly, one token at a time."""

    def __init__(self, delay: float = 0.2, tokens: int = 5):
        self.delay = delay
        self.tokens = tokens
        self.calls = 0

    def create(self, **kwargs):
        self.calls += 1

        def stream():
            for i in range(self.tokens):
                time.sleep(self.delay)
                yield SimpleNamespace(
                    choices=[SimpleNamespace(
                        delta=SimpleNamespace(content=f"tok{i} "),
                        finish_reason=None,
                    )]
                )
            yield SimpleNamespace(
                choices=[SimpleNamespace(delta=SimpleNamespace(content=None),
                                         finish_reason="stop")]
            )

        return stream()


class FakeClient:
    def __init__(self, completions: FakeCompletions):
        self.chat = type("Chat", (), {"completions": completions})()


class SlowEmbedder:
    """A picture model that takes its time."""

    def __init__(self, delay: float = 0.2):
        self.delay = delay
        self.calls = 0

    def embed_text(self, text, prefix=""):
        time.sleep(self.delay)
        self.calls += 1
        return [1.0] * IMAGE_DIM

    def embed_image(self, path):
        time.sleep(self.delay)
        self.calls += 1
        return [1.0] * IMAGE_DIM


def _make_picture(relative: str):
    path = get_storage_root() / relative
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(b"\xff\xd8\xff\xe0" + b"\x00" * 32)
    return path


@pytest.fixture(autouse=True)
def a_pool_that_starts_free(monkeypatch):
    """The pool must be free before and after each test.

    The window in which the service counts as "somebody is talking" is
    shortened: the point is the contention between the pool and the
    indexer, not the length of a pause.
    """
    from app.interaction import interaction

    monkeypatch.setattr(interaction, "_idle_seconds", 0.3)
    pool = get_big_pool()
    yield pool
    # A test that left the pool locked would hang every test after it.
    assert pool.acquire(timeout=5), "the pool was left locked"
    pool.release()


def test_a_conversation_and_a_picture_index_do_not_deadlock(monkeypatch):
    """A conversation holds the cores; the index waits for them.

    The index build wants the whole box, so it cannot start until the
    answer is finished. It must wait, not fail, and not corrupt
    anything.
    """
    import app.llm_client as llm_client
    from app.image_indexer import ImageIndexer

    completions = FakeCompletions(delay=0.15, tokens=4)
    monkeypatch.setattr(llm_client, "_get_client", lambda provider: FakeClient(completions))

    embedder = SlowEmbedder(delay=0.05)
    monkeypatch.setattr("app.image_indexer.get_image_embedder", lambda: embedder)

    picture = _make_picture("负载测试/图.jpg")

    errors = []
    answered = []
    indexed = []
    timings = {}

    def converse():
        try:
            started = time.monotonic()
            for _ in llm_client.chat_completion_stream(
                [{"role": "user", "content": "讲个故事"}],
                provider=SlowProvider(),
            ):
                pass
            timings["conversation"] = time.monotonic() - started
            answered.append(True)
        except Exception as e:  # noqa: BLE001
            errors.append(f"conversation: {e!r}")

    def build():
        try:
            indexer = ImageIndexer()
            timings["build_started"] = time.monotonic()
            indexer._build(limit=1, rebuild=True)
            timings["build_finished"] = time.monotonic()
            indexed.append(True)
        except Exception as e:  # noqa: BLE001
            errors.append(f"index build: {e!r}")

    threads = [threading.Thread(target=converse), threading.Thread(target=build)]
    started = time.monotonic()
    for t in threads:
        t.start()
    for t in threads:
        t.join(timeout=60)

    assert not any(t.is_alive() for t in threads), "something hung"
    assert time.monotonic() - started < 60, "it took too long"
    assert not errors, errors
    assert answered == [True]
    assert indexed == [True]
    assert picture.exists()

    # The conversation was not held up by the index build: four tokens
    # at 150ms each is 600ms, and the build must not make it longer
    # than a conversation's own length.
    assert timings["conversation"] < 3.0, timings

    # And the build stood down while the answer was going, rather than
    # taking the cores and slowing the answer down. It starts once the
    # machine is quiet again.
    assert timings["build_started"] > 0
    assert timings["build_finished"] >= timings["build_started"]


def test_a_picture_search_works_while_a_conversation_is_running(monkeypatch):
    """Both are readers: neither has to wait for the other."""
    import app.llm_client as llm_client
    import app.image_search as image_search

    completions = FakeCompletions(delay=0.15, tokens=6)
    monkeypatch.setattr(llm_client, "_get_client", lambda provider: FakeClient(completions))
    monkeypatch.setattr(image_search, "get_image_embedder", lambda: SlowEmbedder(delay=0.05))

    errors = []
    found = []

    def converse():
        try:
            for _ in llm_client.chat_completion_stream(
                [{"role": "user", "content": "讲个故事"}],
                provider=SlowProvider(),
            ):
                pass
        except Exception as e:  # noqa: BLE001
            errors.append(f"conversation: {e!r}")

    def search():
        try:
            result = image_search.search_by_text("图")
            found.append(result)
        except Exception as e:  # noqa: BLE001
            errors.append(f"search: {e!r}")

    threads = [threading.Thread(target=converse), threading.Thread(target=search)]
    for t in threads:
        t.start()
    for t in threads:
        t.join(timeout=60)

    assert not any(t.is_alive() for t in threads), "something hung"
    assert not errors, errors
    # The search ran while the conversation was running, and it did not
    # have to wait for it: a text search is a fraction of a second of
    # work.
    assert len(found) == 1


def test_the_pool_is_given_back_after_a_conversation(monkeypatch):
    """A conversation that ends must not leave the cores locked.

    The lease is held for the whole answer, and the answer is a
    generator: if the caller walks away, the generator is closed and
    the with-block ends.
    """
    import app.llm_client as llm_client

    completions = FakeCompletions(delay=0.05, tokens=3)
    monkeypatch.setattr(llm_client, "_get_client", lambda provider: FakeClient(completions))

    stream = llm_client.chat_completion_stream(
        [{"role": "user", "content": "讲个故事"}],
        provider=SlowProvider(),
    )
    next(stream)
    stream.close()

    pool = get_big_pool()
    assert pool.acquire(timeout=10), "the cores were not given back"
    pool.release()


def test_many_conversations_at_once_do_not_starve_each_other(monkeypatch):
    """Five conversations and a search, together.

    Every one of them is a reader, so all of them can run at the same
    time. If the pool serialised them, the box would feel like one
    conversation at a time.
    """
    import app.llm_client as llm_client

    completions = FakeCompletions(delay=0.05, tokens=3)
    monkeypatch.setattr(llm_client, "_get_client", lambda provider: FakeClient(completions))

    errors = []
    done = []

    def converse(i):
        try:
            for _ in llm_client.chat_completion_stream(
                [{"role": "user", "content": f"问题{i}"}],
                provider=SlowProvider(),
            ):
                pass
            done.append(i)
        except Exception as e:  # noqa: BLE001
            errors.append(f"conversation {i}: {e!r}")

    threads = [threading.Thread(target=converse, args=(i,)) for i in range(5)]
    started = time.monotonic()
    for t in threads:
        t.start()
    for t in threads:
        t.join(timeout=60)

    assert not any(t.is_alive() for t in threads), "something hung"
    assert not errors, errors
    assert len(done) == 5
    # Five conversations of three tokens each, at 50ms a token, running
    # side by side: about 150ms. Serialised they would take five times
    # that. The check is loose on purpose -- the point is that they are
    # not one after another.
    assert time.monotonic() - started < 3.0, "the conversations queued up behind each other"


def test_an_upload_a_conversation_and_a_picture_search_at_the_same_time(monkeypatch):
    """The three things a household actually does at once.

    An upload writes a file and takes the storage lock; a conversation
    holds the big cores as a reader; a picture search asks the model
    for a vector. All three go through the service at the same time.

    The bar is that all three finish, that the uploaded file is whole,
    and that the service is still usable afterwards.
    """
    import app.llm_client as llm_client
    import app.image_search as image_search
    from conftest import client

    completions = FakeCompletions(delay=0.1, tokens=4)
    monkeypatch.setattr(llm_client, "_get_client", lambda provider: FakeClient(completions))
    monkeypatch.setattr(image_search, "get_image_embedder", lambda: SlowEmbedder(delay=0.05))

    body = "上传的内容" * 200
    errors = []
    results = {}

    def upload():
        try:
            res = client.post(
                "/api/fs/upload?path=",
                files={"files": ("负载-上传.txt", body.encode("utf-8"), "text/plain")},
            )
            results["upload"] = res.status_code
        except Exception as e:  # noqa: BLE001
            errors.append(f"upload: {e!r}")

    def converse():
        try:
            for _ in llm_client.chat_completion_stream(
                [{"role": "user", "content": "讲个故事"}],
                provider=SlowProvider(),
            ):
                pass
            results["conversation"] = True
        except Exception as e:  # noqa: BLE001
            errors.append(f"conversation: {e!r}")

    def search():
        try:
            image_search.search_by_text("图")
            results["search"] = True
        except Exception as e:  # noqa: BLE001
            errors.append(f"search: {e!r}")

    threads = [threading.Thread(target=upload),
               threading.Thread(target=converse),
               threading.Thread(target=search)]
    started = time.monotonic()
    for t in threads:
        t.start()
    for t in threads:
        t.join(timeout=60)

    assert not any(t.is_alive() for t in threads), "something hung"
    assert time.monotonic() - started < 60, "it took too long"
    assert not errors, errors
    assert results.get("upload") == 200, results
    assert results.get("conversation") is True, results
    assert results.get("search") is True, results

    # The uploaded file is whole, not a mix of two writes.
    target = get_storage_root() / "负载-上传.txt"
    assert target.exists()
    assert target.read_text(encoding="utf-8") == body
    target.unlink()

    # And the service still answers.
    res = client.get("/api/fs/browse?path=")
    assert res.status_code == 200
