"""The picture vector table.

Pictures live in a table of their own. jina-embeddings-v5-omni puts images
and text in one vector space -- that is what makes "find me a picture of a
red apple" work -- but its vectors are 1024 wide while bge's are 768, and a
LanceDB table has a single width for its vector column. Keeping them apart
also means the nine thousand document vectors that are already indexed do
not have to be thrown away and rebuilt.

One row per picture. The row id is derived from the path, so re-encoding a
picture replaces its row instead of adding a second one.
"""

import hashlib
import os
import re
import threading
import time
from pathlib import Path
from typing import Optional

IMAGE_TABLE = "image_embeddings"

# The width of jina-embeddings-v5-omni-small's vectors.
IMAGE_DIM = 1024


class ImageVectorStore:
    """A LanceDB table of picture vectors."""

    def __init__(self, db_path: str, dim: int = IMAGE_DIM):
        import lancedb
        import pyarrow as pa

        self.db_path = db_path
        self.dim = dim
        self.db = lancedb.connect(db_path)
        self._ensure_table(pa)
        self._table = self._open_table()

    def _open_table(self):
        """Open the table, waiting for it to become visible.

        A table another connection just created is not visible to this one
        for a moment.
        """
        last = None
        for attempt in range(6):
            try:
                return self.db.open_table(IMAGE_TABLE)
            except Exception as e:  # noqa: BLE001
                last = e
                time.sleep(0.05 * (attempt + 1))
        raise RuntimeError(f"打不开图片向量表 {IMAGE_TABLE}: {last}")

    def _ensure_table(self, pa) -> None:
        if IMAGE_TABLE in self.db.table_names():
            return
        schema = pa.schema([
            ("id", pa.string()),
            ("file_path", pa.string()),
            ("vector", pa.list_(pa.float32(), self.dim)),
            ("content_hash", pa.string()),
            ("file_size", pa.int64()),
            ("modified_at", pa.float64()),
        ])
        for attempt in range(6):
            if IMAGE_TABLE in self.db.table_names():
                return
            try:
                self.db.create_table(IMAGE_TABLE, schema=schema, mode="create")
                return
            except Exception as e:  # noqa: BLE001
                message = str(e).lower()
                if "already exists" in message:
                    # Another thread or process created it between the
                    # check and here. That is the normal case on a first
                    # run, not an error.
                    time.sleep(0.05)
                    continue
                if "conflict" in message or "preempted" in message:
                    # Two connections committed at the same time. The
                    # loser retries and finds the table there.
                    time.sleep(0.1 * (attempt + 1))
                    continue
                raise
        if IMAGE_TABLE not in self.db.table_names():
            raise RuntimeError(f"无法建立图片向量表 {IMAGE_TABLE}")

    # -- writing -----------------------------------------------------------

    def add(
        self,
        vector: list[float],
        file_path: str,
        content_hash: str = "",
        file_size: int = 0,
        modified_at: float = 0.0,
    ) -> None:
        self.add_many([vector], [{
            "file_path": file_path,
            "content_hash": content_hash,
            "file_size": file_size,
            "modified_at": modified_at,
        }])

    def add_many(self, vectors: list[list[float]], records: list[dict]) -> None:
        """Insert or replace rows.

        LanceDB's add is append-only, even for a row that is already there,
        so the id has to be stable and merge_insert is what does the
        replacing.
        """
        rows = []
        for vec, rec in zip(vectors, records):
            path = rec.get("file_path", "")
            rows.append({
                "id": hashlib.sha256(path.encode()).hexdigest(),
                "file_path": path,
                "vector": vec,
                "content_hash": rec.get("content_hash", ""),
                "file_size": int(rec.get("file_size", 0)),
                "modified_at": float(rec.get("modified_at", 0.0)),
            })
        if rows:
            (self._table.merge_insert("id")
             .when_matched_update_all()
             .when_not_matched_insert_all()
             .execute(rows))

    def delete_by_file(self, file_path: str) -> None:
        """Remove the row for one picture."""
        ids = self._ids_for([file_path])
        if ids:
            self._table.delete(" OR ".join(f"id = '{i}'" for i in ids))

    def delete_missing(self, known_paths: set[str]) -> int:
        """Drop rows whose file is gone. Returns how many were removed."""
        ids = [i for i in self._ids_for_all() if i[1] not in known_paths]
        if not ids:
            return 0
        self._table.delete(" OR ".join(f"id = '{i}'" for i, _ in ids))
        return len(ids)

    def cleanup_old_versions(self, older_than_days: int = 2) -> dict:
        """Drop the table's superseded versions.

        Every write leaves the previous version of the table on disk, and
        merge_insert writes one version per picture. Without this the
        version directory grows without bound -- the text indexer hit
        exactly this and needed its own cleaner.
        """
        from datetime import timedelta

        try:
            stats = self._table.cleanup_old_versions(
                older_than=timedelta(days=older_than_days)
            )
            return {
                "ok": True,
                "removed_versions": getattr(stats, "bytes_removed", None)
                and int(stats.bytes_removed) or 0,
                "detail": str(stats)[:200],
            }
        except Exception as e:
            return {"ok": False, "detail": f"{type(e).__name__}: {str(e)[:150]}"}

    # -- reading -----------------------------------------------------------

    def search(self, query_vector: list[float], top_k: int = 12) -> list[dict]:
        """The pictures closest to the query, best first.

        The score is a cosine similarity rather than the distance LanceDB
        hands back: it reports the *squared* distance, and for unit-length
        vectors that is 2 - 2*cos, so halving it turns the number into
        something a person can read ("相似度 0.37").
        """
        if self.count() == 0:
            return []
        rows = (self._table.search(query_vector, vector_column_name="vector")
                .limit(top_k).to_list())
        results = []
        for r in rows:
            distance = float(r.get("_distance", 0.0))
            cosine = 1.0 - distance / 2.0
            results.append({"score": max(-1.0, min(1.0, cosine)), **r})
        return results

    def count(self) -> int:
        try:
            return self._table.count_rows()
        except Exception:
            try:
                return self._table.to_arrow().num_rows
            except Exception:
                return 0

    def indexed(self) -> dict[str, str]:
        """Path -> content hash for everything in the table."""
        try:
            rows = self._table.to_arrow().select(["file_path", "content_hash"]).to_pylist()
        except Exception:
            return {}
        return {r.get("file_path", ""): r.get("content_hash", "") for r in rows}

    def needs_encoding(self, file_path: str, content_hash: str) -> bool:
        """Whether this picture has to be encoded again.

        The hash covers the bytes, so an edited picture is re-encoded while
        a touched-but-unchanged one is not.
        """
        return self.indexed().get(file_path) != content_hash

    def clear(self) -> None:
        if IMAGE_TABLE in self.db.table_names():
            self.db.drop_table(IMAGE_TABLE)
        import pyarrow as pa
        self._ensure_table(pa)
        self._table = self.db.open_table(IMAGE_TABLE)

    # -- internals ---------------------------------------------------------

    def _ids_for(self, paths: list[str]) -> list[str]:
        wanted = set(paths)
        return [i for i, p in self._ids_for_all() if p in wanted]

    def _ids_for_all(self) -> list[tuple[str, str]]:
        """(id, path) for every row.

        The path is user-controlled, so the ids -- which are SHA-256 hex and
        therefore server-controlled -- are the only thing that goes into the
        WHERE clause.
        """
        try:
            rows = self._table.to_arrow().select(["id", "file_path"]).to_pylist()
        except Exception:
            return []
        return [(r.get("id", ""), r.get("file_path", "")) for r in rows
                if re.fullmatch(r"[a-f0-9]{64}", r.get("id", "") or "")]


def file_hash(path: Path | str, chunk_size: int = 1 << 20) -> str:
    """A hash of the file's bytes, used to spot edits."""
    h = hashlib.sha256()
    with open(path, "rb") as f:
        while True:
            chunk = f.read(chunk_size)
            if not chunk:
                break
            h.update(chunk)
    return h.hexdigest()


_stores: dict[str, tuple[ImageVectorStore, tuple]] = {}
_stores_lock = threading.Lock()


def _directory_mark(db_path: str) -> tuple:
    """What the table's directory looks like right now.

    A handle to a table whose directory was deleted and recreated still
    reads the old rows and refuses to write. The identity of the directory
    catches that.
    """
    try:
        st = os.stat(db_path)
        return (st.st_dev, st.st_ino)
    except OSError:
        return (0, 0)


def get_image_store(db_path: str) -> ImageVectorStore:
    """The picture table, cached per path.

    Opening a LanceDB table is not free, and the page asks for the state of
    the index every few seconds. The cache is dropped when the directory
    underneath it has been replaced.
    """
    with _stores_lock:
        mark = _directory_mark(db_path)
        cached = _stores.get(db_path)
        if cached is not None and cached[1] == mark:
            return cached[0]
        store = ImageVectorStore(db_path)
        # The mark is taken again: creating the table makes the directory.
        _stores[db_path] = (store, _directory_mark(db_path))
        return store
