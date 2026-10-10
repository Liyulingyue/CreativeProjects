"""Hybrid search: SQLite FTS5 (BM25) + LanceDB vector search, merged with RRF.

The FTS side uses the trigram tokenizer so Chinese and substring queries work
without a dedicated segmentation library.
"""

import sqlite3
from pathlib import Path

from .chunking import chunk_file
from .deps import DATA_DIR, state

# A long document matches in many passages; without a cap its chunks would
# fill the whole result list. Applied after ranking, not before.
MAX_HITS_PER_FILE = 3


class HybridSearchEngine:
    def __init__(self, db_path: str):
        self.db_path = db_path
        self._init_db()

    def _get_conn(self) -> sqlite3.Connection:
        return sqlite3.connect(self.db_path, check_same_thread=False)

    def _init_db(self):
        conn = self._get_conn()
        # An existing index built before chunking has no chunk_idx column.
        # CREATE TABLE IF NOT EXISTS would silently keep the old shape and
        # every insert would then fail, so the old table is dropped and the
        # index is rebuilt from the manifest on the next scan.
        cols = [r[1] for r in conn.execute("PRAGMA table_info(fts_files)")]
        if cols and "chunk_idx" not in cols:
            conn.execute("DROP TABLE fts_files")
            cols = []
        if not cols:
            conn.execute("""
                CREATE VIRTUAL TABLE IF NOT EXISTS fts_files USING fts5(
                    path UNINDEXED, name, content, chunk_idx UNINDEXED, tokenize='trigram'
                )
            """)
        conn.commit()
        conn.close()

    def upsert_file(self, path: str, content: str):
        """Store a file as one row per chunk.

        Documents are indexed in chunks because a whole novel exceeds the
        embedding model's context and retrieves as a single diluted vector;
        chunk rows also let a keyword hit name the passage, not just the file.
        """
        name = Path(path).name
        pieces = chunk_file(content)
        conn = self._get_conn()
        conn.execute("DELETE FROM fts_files WHERE path = ?", (path,))
        conn.executemany(
            "INSERT INTO fts_files (path, name, content, chunk_idx) VALUES (?, ?, ?, ?)",
            [(path, name, text, idx) for idx, (_, text) in enumerate(pieces)],
        )
        conn.commit()
        conn.close()

    def file_chunk_count(self, path: str) -> int:
        conn = self._get_conn()
        row = conn.execute("SELECT COUNT(*) FROM fts_files WHERE path = ?", (path,)).fetchone()
        conn.close()
        return row[0] if row else 0

    def remove_file(self, path: str):
        conn = self._get_conn()
        conn.execute("DELETE FROM fts_files WHERE path = ?", (path,))
        conn.commit()
        conn.close()

    def clear(self):
        conn = self._get_conn()
        conn.execute("DELETE FROM fts_files")
        conn.commit()
        conn.close()

    def count(self) -> int:
        conn = self._get_conn()
        row = conn.execute("SELECT COUNT(*) FROM fts_files").fetchone()
        conn.close()
        return row[0] if row else 0

    def bm25_search(self, query: str, top_k: int = 10) -> list[dict]:
        """Keyword search over chunks.

        Hits are returned per chunk, not per file: the two channels index the
        same chunks, so a keyword hit and a semantic hit can be merged onto the
        same passage instead of the same document.
        """
        if not query.strip():
            return []
        conn = self._get_conn()
        try:
            rows = conn.execute(
                """SELECT path, name, chunk_idx,
                          snippet(fts_files, 2, '<<', '>>', '…', 24) AS preview,
                          bm25(fts_files) AS rank
                   FROM fts_files
                   WHERE fts_files MATCH ?
                   ORDER BY rank LIMIT ?""",
                (query, max(top_k * 5, 25))
            ).fetchall()
        except sqlite3.OperationalError:
            rows = []
        finally:
            conn.close()

        return [
            {
                "path": path,
                "name": name,
                "chunk_idx": chunk_idx,
                "preview": preview,
                "score": 1.0 - rank,  # bm25 rank is negative; lower = better
                "source": "keyword",
            }
            for path, name, chunk_idx, preview, rank in rows
        ]

    def vector_search(self, query: str, top_k: int = 10) -> list[dict]:
        try:
            rag_svc = state.get_rag_service()
            query_vector = rag_svc.embedding_service.embed([query])[0]
            results = rag_svc.vector_store.search(query_vector, top_k=top_k)
            return [
                {
                    "path": r.get("file_path", ""),
                    "name": Path(r.get("file_path", "")).name,
                    "chunk_idx": int(r.get("chunk_idx", 0) or 0),
                    "preview": str(r.get("content", ""))[:200],
                    "score": r.get("score", 0),
                    "source": "semantic",
                }
                for r in results
            ]
        except Exception:
            return []

    def hybrid_search(self, query: str, top_k: int = 10) -> dict:
        fts_results = self.bm25_search(query, top_k)
        vec_results = self.vector_search(query, top_k)

        # Reciprocal Rank Fusion keyed on (path, chunk_idx): both channels index
        # the same chunks, so agreement on one passage is a much stronger signal
        # than two hits on different passages of the same book.
        rrf: dict[tuple[str, int], dict] = {}
        for channel in (fts_results, vec_results):
            for rank, r in enumerate(channel, 1):
                key = (r["path"], r["chunk_idx"])
                entry = rrf.setdefault(key, {
                    "path": r["path"], "name": r["name"], "chunk_idx": r["chunk_idx"],
                    "preview": r["preview"], "score": 0.0, "sources": [],
                })
                entry["score"] += 1.0 / (60 + rank)
                if r["source"] not in entry["sources"]:
                    entry["sources"].append(r["source"])
                # Prefer the keyword snippet, which is built around the match.
                if r["source"] == "keyword":
                    entry["preview"] = r["preview"]

        ranked = sorted(rrf.values(), key=lambda x: x["score"], reverse=True)

        # Per-file cap applied *after* chunk-level ranking: a 400-chunk novel
        # matches everywhere, and without this it would crowd out everything
        # else even though its individual chunks rank lower.
        merged: list[dict] = []
        per_file: dict[str, int] = {}
        for item in ranked:
            if per_file.get(item["path"], 0) >= MAX_HITS_PER_FILE:
                continue
            per_file[item["path"]] = per_file.get(item["path"], 0) + 1
            merged.append(item)
            if len(merged) >= top_k:
                break

        return {
            "results": merged,
            "keyword_count": len(fts_results),
            "semantic_count": len(vec_results),
            "query": query,
        }


search_engine = HybridSearchEngine(str(DATA_DIR / "search.db"))
