"""The providers list: the one main provider, and the rows around it.

Two things have to hold at all times:

* there is a main provider, because a new chat session needs one to
  land on;
* a provider can be created without filling in every field, because
  the fields that have a default should use it.
"""

import threading

import pytest

from conftest import client

from app.deps import state


def _make(name="模型", **kwargs):
    svc = state.get_chat_history()
    provider = svc.create_provider(
        name, kwargs.pop("base_url", "http://127.0.0.1:9010/v1"),
        kwargs.pop("model", "m"), **kwargs)
    return provider


@pytest.fixture
def providers():
    svc = state.get_chat_history()
    before = {p.id for p in svc.list_providers()}
    yield svc
    # Leave the table as it was.
    for p in svc.list_providers():
        if p.id not in before:
            svc.delete_provider(p.id, keep_a_main=False)


class TestAMainProviderAlwaysRemains:
    def test_the_only_main_cannot_be_demoted(self):
        """Demoting the last main is refused, and says why."""
        svc = state.get_chat_history()
        main = [p for p in svc.list_providers() if p.is_main]
        if not main:
            pytest.skip("this installation has no main provider")
        res = client.patch(f"/api/providers/{main[0].id}", json={"is_main": False})
        assert res.status_code == 400, res.text
        assert "main" in res.json()["detail"]
        # And it is still there.
        fresh = svc.get_provider(main[0].id)
        assert fresh.is_main

    def test_the_only_main_cannot_be_deleted(self):
        svc = state.get_chat_history()
        main = [p for p in svc.list_providers() if p.is_main]
        if not main:
            pytest.skip("this installation has no main provider")
        res = client.delete(f"/api/providers/{main[0].id}")
        assert res.status_code == 400, res.text
        assert svc.get_provider(main[0].id) is not None

    def test_two_administrators_at_once_cannot_leave_none(self):
        """The check and the change have to be together.

        If they are separate, two administrators can each see "there
        is another main", each demote theirs, and the system is left
        with no main at all.
        """
        svc = state.get_chat_history()
        a = _make("甲")
        b = _make("乙")
        svc.update_provider(a.id, is_main=True)

        results = []
        barrier = threading.Barrier(2)

        def demote(provider_id, label):
            barrier.wait()
            res = client.patch(f"/api/providers/{provider_id}",
                               json={"is_main": False})
            results.append((label, res.status_code))

        threads = [
            threading.Thread(target=demote, args=(a.id, "甲")),
            threading.Thread(target=demote, args=(b.id, "乙")),
        ]
        for t in threads:
            t.start()
        for t in threads:
            t.join(timeout=30)
        assert not any(t.is_alive() for t in threads), "the request hung"

        mains = [p for p in svc.list_providers() if p.is_main]
        assert mains, f"no main provider left: {results}"


class TestCreatingAProvider:
    def test_a_provider_can_be_created_without_a_thinking_budget(self):
        """The field has a default, so leaving it out should work.

        The column does not accept a null, and the value that was
        handed in was a null -- so creating a provider without that
        field failed with a database error.
        """
        res = client.post("/api/providers", json={
            "name": "新模型",
            "base_url": "http://127.0.0.1:9010/v1",
            "model": "m",
        })
        assert res.status_code == 201, res.text
        body = res.json()
        assert body["reasoning_budget"] == 1024
        # And it can be deleted again, so the test leaves nothing behind.
        assert client.delete(f"/api/providers/{body['id']}").status_code == 200

    def test_a_provider_can_be_created_without_a_token_limit(self):
        res = client.post("/api/providers", json={
            "name": "又一个模型",
            "base_url": "http://127.0.0.1:9010/v1",
            "model": "m",
            "reasoning_budget": 512,
        })
        assert res.status_code == 201, res.text
        assert res.json()["max_tokens"] is None
        client.delete(f"/api/providers/{res.json()['id']}")


class TestTheEmbeddingProviders:
    """The embedding side has the same two rules as the LLM side.

    There has to be an index provider -- the picture index builds into
    it -- and a provider can be created without filling in every field.
    """

    @pytest.fixture
    def embeddings(self):
        from app.deps import state

        svc = state.get_chat_history()
        before = {p.id for p in svc.list_embedding_providers()}
        yield svc
        for p in svc.list_embedding_providers():
            if p.id not in before:
                svc.delete_embedding_provider(p.id, keep_an_index=False)

    def test_the_only_index_provider_cannot_be_demoted(self, embeddings):
        """Demoting the last index provider is refused."""
        index = [p for p in embeddings.list_embedding_providers() if p.is_index]
        if not index:
            pytest.skip("this installation has no index provider")
        res = client.patch(f"/api/embedding_providers/{index[0].id}",
                           json={"is_index": False})
        assert res.status_code == 400, res.text
        assert embeddings.get_embedding_provider(index[0].id).is_index

    def test_the_only_index_provider_cannot_be_deleted(self, embeddings):
        index = [p for p in embeddings.list_embedding_providers() if p.is_index]
        if not index:
            pytest.skip("this installation has no index provider")
        res = client.delete(f"/api/embedding_providers/{index[0].id}")
        assert res.status_code == 400, res.text
        assert embeddings.get_embedding_provider(index[0].id) is not None

    def test_two_administrators_at_once_cannot_leave_none(self, embeddings):
        """The check and the change have to be together here too."""
        a = embeddings.create_embedding_provider(
            "甲", "http://127.0.0.1:9011/v1", "m1", dim="128")
        b = embeddings.create_embedding_provider(
            "乙", "http://127.0.0.1:9012/v1", "m2", dim="128")
        embeddings.update_embedding_provider(a.id, is_index=True)

        results = []
        barrier = threading.Barrier(2)

        def demote(provider_id, label):
            barrier.wait()
            res = client.patch(f"/api/embedding_providers/{provider_id}",
                               json={"is_index": False})
            results.append((label, res.status_code))

        threads = [
            threading.Thread(target=demote, args=(a.id, "甲")),
            threading.Thread(target=demote, args=(b.id, "乙")),
        ]
        for t in threads:
            t.start()
        for t in threads:
            t.join(timeout=30)
        assert not any(t.is_alive() for t in threads), "the request hung"

        index = [p for p in embeddings.list_embedding_providers() if p.is_index]
        assert index, f"no index provider left: {results}"

    def test_a_provider_can_be_created_without_a_dimension(self):
        """The dimension has a default, so leaving it out should work."""
        res = client.post("/api/embedding_providers", json={
            "name": "新模型",
            "base_url": "http://127.0.0.1:9011/v1",
            "model": "m",
        })
        assert res.status_code == 201, res.text
        body = res.json()
        assert body["dim"] == "AUTO", body
        client.delete(f"/api/embedding_providers/{body['id']}")
