"""Regression tests for vector row identity and replacement semantics."""

from app.deps import LanceDBVectorStore, RAGService


def test_add_many_merges_same_file_chunk(tmp_path, monkeypatch):
    from app import deps

    monkeypatch.setitem(deps.EMBEDDING_DIM_MAP, "test-embed", 2)
    store = LanceDBVectorStore(str(tmp_path / "vectors"))
    store.add_many(
        [[1.0, 0.0]],
        [{"file_path": "/root/a.txt", "chunk_idx": 0, "content": "old"}],
    )
    store.add_many(
        [[0.0, 1.0]],
        [{"file_path": "/root/a.txt", "chunk_idx": 0, "content": "new"}],
    )

    rows = store._table.to_arrow().to_pylist()
    assert len(rows) == 1
    assert rows[0]["content"] == "new"
    assert rows[0]["vector"] == [0.0, 1.0]


def test_index_files_deletes_stale_tail_chunks(tmp_path, monkeypatch):
    from app import deps

    monkeypatch.setitem(deps.EMBEDDING_DIM_MAP, "test-embed", 2)
    store = LanceDBVectorStore(str(tmp_path / "vectors"))

    class FakeEmbedding:
        def embed(self, texts):
            return [[float(len(text)), 1.0] for text in texts]

    rag = RAGService(FakeEmbedding(), "", "", "", str(tmp_path / "vectors"))
    rag.vector_store = store
    rag.index_file("/root/a.txt", "first passage. " * 500)
    old_count = store.count()
    assert old_count > 1

    rag.index_file("/root/a.txt", "short")
    rows = store._table.to_arrow().to_pylist()
    assert len(rows) == 1
    assert rows[0]["file_path"] == "/root/a.txt"
    assert rows[0]["chunk_idx"] == 0
    assert rows[0]["content"] == "short"
