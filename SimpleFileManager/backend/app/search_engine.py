"""Hybrid search: SQLite FTS5 (BM25) + LanceDB vector search, merged with RRF.

The FTS side uses the trigram tokenizer so Chinese and substring queries work
without a dedicated segmentation library.
"""

import sqlite3
from pathlib import Path

from .deps import DATA_DIR, state


class HybridSearchEngine:
    def __init__(self, db_path: str):
        self.db_path = db_path
        self._init_db()

    def _get_conn(self) -> sqlite3.Connection:
        return sqlite3.connect(self.db_path, check_same_thread=False)

    def _init_db(self):
        conn = self._get_conn()
        conn.execute("""
            CREATE VIRTUAL TABLE IF NOT EXISTS fts_files USING fts5(
                path UNINDEXED, name, content, tokenize='trigram'
            )
        """)
        conn.commit()
        conn.close()

    def upsert_file(self, path: str, content: str):
        name = Path(path).name
        conn = self._get_conn()
        conn.execute("DELETE FROM fts_files WHERE path = ?", (path,))
        conn.execute(
            "INSERT INTO fts_files (path, name, content) VALUES (?, ?, ?)",
            (path, name, content)
        )
        conn.commit()
        conn.close()

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
        if not query.strip():
            return []
        conn = self._get_conn()
        try:
            rows = conn.execute(
                """SELECT path, name, snippet(fts_files, 2, '<<', '>>', '…', 24) AS preview,
                          bm25(fts_files) AS rank
                   FROM fts_files
                   WHERE fts_files MATCH ?
                   ORDER BY rank LIMIT ?""",
                (query, top_k)
            ).fetchall()
        except sqlite3.OperationalError:
            rows = []
        finally:
            conn.close()
        # bm25 rank is negative (lower = better); normalize to a positive score
        return [
            {"path": r[0], "name": r[1], "preview": r[2], "score": 1.0 - r[3], "source": "keyword"}
            for r in rows
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

        # Reciprocal Rank Fusion over both channels
        rrf = {}
        for channel in (fts_results, vec_results):
            for rank, r in enumerate(channel, 1):
                entry = rrf.setdefault(r["path"], {
                    "path": r["path"], "name": r["name"], "preview": r["preview"],
                    "score": 0.0, "sources": [],
                })
                entry["score"] += 1.0 / (60 + rank)
                if r["source"] not in entry["sources"]:
                    entry["sources"].append(r["source"])

        merged = sorted(rrf.values(), key=lambda x: x["score"], reverse=True)[:top_k]
        return {
            "results": merged,
            "keyword_count": len(fts_results),
            "semantic_count": len(vec_results),
            "query": query,
        }


search_engine = HybridSearchEngine(str(DATA_DIR / "search.db"))
