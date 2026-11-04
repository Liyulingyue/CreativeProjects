import json
import hashlib
import os
import time
import sqlite3
import threading
import uuid
from contextvars import ContextVar
from pathlib import Path
from typing import Any, Optional

from dotenv import load_dotenv
from .chunking import chunk_file
from .interaction import interaction
from . import db
from .users import USERS_DB_NAME
from .versions import VERSIONS_DIR_NAME
from .models import (
    AppSettings, IndexStats, IndexStatus, ChatSession, ChatMessage,
    AgentPlan, PlanAction, LLMProvider, EmbeddingProvider,
    DEFAULT_REASONING_BUDGET,
)

load_dotenv()

BASE_DIR = Path(__file__).resolve().parent.parent
STORAGE_PATH = os.getenv("STORAGE_PATH", "./data")
ROOT_DIR = BASE_DIR / STORAGE_PATH
ROOT_DIR.mkdir(exist_ok=True)
DATA_DIR = ROOT_DIR / ".simplefilemanager"
DATA_DIR.mkdir(exist_ok=True)

SETTINGS_FILE = DATA_DIR / "settings.json"

# Marker for "the caller did not mention this field", so a nullable
# column can still be cleared explicitly.
_UNSET = object()
INDEX_STATS_FILE = DATA_DIR / "index_stats.json"

# Chunks per embedding call. Deliberately small: a batch is uninterruptible,
# so a large one keeps the embedding server busy long after the indexer has
# noticed the user started a conversation. At 16 the batch ran ~60s.
# Embedding one 4-chunk batch costs ~26s of prompt processing on this box, and
# most of that is fixed per-call overhead: 12 chunks in one call took 0.44x the
# time of 4, so throughput scales well past 4. Kept interruptible -- the indexer
# checks for interaction/pause between batches, so a large batch only delays
# yielding, it does not block it.
EMBED_CHUNK_BATCH = 12

EMBEDDING_DIM_MAP = {
    "text-embedding-3-small": 1536,
    "text-embedding-3-large": 3072,
    "text-embedding-ada-002": 1536,
}


def _detect_embedding_dim(model: str, api_key: str, base_url: str) -> int:
    try:
        from .llm_client import get_embedding
        vec = get_embedding("test")
        if vec:
            return len(vec)
    except Exception:
        pass
    for known_model, dim in EMBEDDING_DIM_MAP.items():
        if model.startswith(known_model):
            return dim
    return 1536


def _load_json(path: Path, default=None):
    if not path.exists():
        return default if default is not None else {}
    with open(path, "r", encoding="utf-8") as f:
        return json.load(f)


def _save_json(path: Path, data):
    path.parent.mkdir(parents=True, exist_ok=True)
    with open(path, "w", encoding="utf-8") as f:
        json.dump(data, f, indent=2, ensure_ascii=False)


def _update_env_value(path: Path, key: str, value: str) -> None:
    """Idempotently set `key=value` in a .env-style file.

    Preserves comments, blank lines, and key ordering. If the key is
    already present (in any line starting with optional whitespace) it
    gets its value rewritten in place; otherwise the key is appended at
    the end. No quoting/escaping — values are written verbatim, which is
    fine for our booleans, ports, and URLs. The .env is intentionally
    hand-edited, so we never silently rewrite user comments.
    """
    path.parent.mkdir(parents=True, exist_ok=True)
    lines: list[str] = []
    replaced = False
    if path.exists():
        with open(path, "r", encoding="utf-8") as f:
            for line in f:
                stripped = line.lstrip()
                if not replaced and stripped.startswith(f"{key}="):
                    lines.append(f"{key}={value}\n")
                    replaced = True
                else:
                    lines.append(line)
    if not replaced:
        # Ensure a trailing newline before appending, so the new key sits
        # on its own line regardless of whether the file ended with one.
        if lines and not lines[-1].endswith("\n"):
            lines[-1] = lines[-1] + "\n"
        lines.append(f"{key}={value}\n")
    with open(path, "w", encoding="utf-8") as f:
        f.writelines(lines)


def _calc_storage_used() -> int:
    total = 0
    for root, dirs, files in os.walk(DATA_DIR):
        for f in files:
            fp = os.path.join(root, f)
            try:
                total += os.path.getsize(fp)
            except (PermissionError, OSError):
                pass
    return total


class EmbeddingService:
    def __init__(self, api_key: str, base_url: str, model: str):
        self.api_key = api_key
        self.base_url = base_url
        self.model = model

    def embed(self, texts: list[str]) -> list[list[float]]:
        # Pass our own URL/model through. llm_client.get_embeddings falls
        # back to state.get_settings() when these are None, so the indexer
        # (which constructs EmbeddingService from the indexer URL) keeps its
        # old behaviour, while the search embedder constructed from
        # EMBEDDING_SEARCH_* actually hits the dedicated endpoint.
        from .llm_client import get_embeddings
        return get_embeddings(
            texts, base_url=self.base_url, api_key=self.api_key, model=self.model,
        )


class LanceDBVectorStore:
    def __init__(self, db_path: str, dim: Optional[int] = None):
        """Open (or create) the vector table.

        `dim` fixes the width of the vector column. Left out, it is
        taken from the index embedding provider, or probed from the
        model -- which needs a running embedding service. A caller that
        knows the width can say so instead, which is what the tests do.
        """
        import lancedb
        self.db_path = db_path
        self.dim = dim
        self.db = lancedb.connect(db_path)
        self._table = None
        self._ensure_table()

    def _ensure_table(self):
        import lancedb
        import pyarrow as pa
        if "file_embeddings" not in self.table_names():
            # LanceDB's vector search accepts a fixed-size list. Keep the
            # configured dimension in the schema so a service restart cannot
            # silently create a table with a different shape.
            #
            # Dim comes from the is_index embedding provider (the
            # indexer-side row). "AUTO" resolves to a probe of the
            # model. Known models short-circuit the probe so a cold
            # start without network still picks a sane dim.
            dim = self.dim or 0
            if not dim:
                idx = state.get_chat_history().get_index_embedding_provider()
            else:
                idx = None
            if idx is not None:
                dim_str = idx.dim or "AUTO"
                if dim_str != "AUTO":
                    try:
                        dim = int(dim_str)
                    except ValueError:
                        dim = 0
                else:
                    dim = EMBEDDING_DIM_MAP.get(idx.model, 0)
            if not dim:
                try:
                    dim = len(self._probe_dim())
                except Exception:
                    dim = 1024
            schema = pa.schema([
                ("id", pa.string()),
                ("file_path", pa.string()),
                ("content", pa.string()),
                ("chunk_idx", pa.int32()),
                ("vector", pa.list_(pa.float32(), dim)),
            ])
            self.db.create_table("file_embeddings", schema=schema, mode="create")
        self._table = self.db.open_table("file_embeddings")

    def _probe_dim(self) -> list[float]:
        from .llm_client import get_embedding
        return get_embedding("dim probe")

    def table_names(self) -> list[str]:
        return self.db.table_names()

    def add(self, vector: list[float], metadata: dict):
        self.add_many([vector], [metadata])

    def add_many(self, vectors: list[list[float]], records: list[dict]):
        # LanceDB Table.add is append-only, even for a duplicate id. Use a
        # deterministic id AND merge_insert: the id makes the key stable while
        # merge_insert is what actually replaces an existing row.
        rows = [{
            "id": f"{hashlib.sha256(r.get('file_path', '').encode()).hexdigest()}#{int(r.get('chunk_idx', 0))}",
            "file_path": r.get("file_path", ""),
            "content": r.get("content", ""),
            "chunk_idx": int(r.get("chunk_idx", 0)),
            "vector": vec,
        } for vec, r in zip(vectors, records)]
        if rows:
            (self._table.merge_insert("id")
             .when_matched_update_all()
             .when_not_matched_insert_all()
             .execute(rows))

    def cleanup_old_versions(self, older_than_days: int = 2) -> dict:
        """Delete the table's superseded versions.

        Every write leaves the previous version of the table on disk.
        The indexer writes on every scan, so the versions directory
        grows without bound -- two thousand files and 240MB for four
        books, of which the live data was a fraction. LanceDB knows
        which versions are still reachable; this asks it to drop the
        rest.

        Versions newer than the threshold are left alone, because a
        version that recent might still be the one a reader is using.
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

    def search(self, query_vector: list[float], top_k: int = 5) -> list[dict]:
        if self.count() == 0:
            return []
        results = self._table.search(query_vector, vector_column_name="vector").limit(top_k).to_list()
        return [{"score": 1.0 - r.get("_distance", 0), **r} for r in results]

    def delete_by_file(self, file_path: str):
        """Delete all chunks for a path. Uses Python-side filtering — the
        path is user-controlled and could contain SQL-dangerous
        characters. IDs are SHA-256 hex (server-controlled) so the
        final WHERE clause is safe."""
        import re as _re
        try:
            rows = self._table.to_arrow().to_pylist()
        except Exception:
            return
        ids = [r["id"] for r in rows if r.get("file_path") == file_path]
        valid = [i for i in ids if _re.fullmatch(r"[a-f0-9]+#[0-9]+", i)]
        if valid:
            self._table.delete(" OR ".join(f"id = '{i}'" for i in valid))

    def rename_file(self, old: str, new: str):
        """Rewrite the file_path column for every chunk of a renamed file.

        Move-detection pass: when the indexer recognises that the same
        content has appeared at a new path, we update the path column in
        place rather than delete-then-reinsert. This preserves the
        vector index intact.

        The row id has to move with it. add_many derives the id from
        sha256(file_path), so a renamed row keeps an id that encodes the
        *old* path. A later re-embed then computes the id for the new
        path, finds no matching key, and inserts a second row for every
        chunk -- the file ends up indexed twice and every search returns
        each passage twice. Rewriting the id keeps the key consistent
        with the path it encodes.
        """
        try:
            rows = (self._table.to_arrow()
                    .select(["id", "file_path", "content", "chunk_idx", "vector"])
                    .to_pylist())
        except Exception:
            rows = []
        moved = [r for r in rows if r.get("file_path") == old]
        if not moved:
            return
        # Re-key and re-insert. The vectors travel with the rows, so this
        # costs no embedding work.
        new_digest = hashlib.sha256(new.encode()).hexdigest()
        rekeyed = [{
            "id": f"{new_digest}#{int(r.get('chunk_idx', 0))}",
            "file_path": new,
            "content": r.get("content", ""),
            "chunk_idx": int(r.get("chunk_idx", 0)),
            "vector": r.get("vector"),
        } for r in moved]
        (self._table.merge_insert("id")
         .when_matched_update_all()
         .when_not_matched_insert_all()
         .execute(rekeyed))

    def clear(self):
        if "file_embeddings" in self.table_names():
            self.db.drop_table("file_embeddings")
        self._ensure_table()

    def count(self) -> int:
        try:
            return self._table.count_rows()
        except Exception:
            if "file_embeddings" in self.table_names():
                # Arrow only; pandas is not a dependency of this project.
                return self._table.to_arrow().num_rows
            return 0

    def list_files(self) -> list[dict]:
        """One entry per indexed file, not per chunk.

        pyarrow is already a dependency and pandas is not, so the table is read
        through Arrow rather than to_pandas() — that raised "No module named
        'pandas'" as soon as the vector store held any row. Rows are collapsed
        by file so a chunked document appears once, with its chunk count.
        """
        if "file_embeddings" not in self.table_names() or self.count() == 0:
            return []
        rows = self._table.to_arrow().select(["id", "file_path", "content"]).to_pylist()

        merged: dict[str, dict] = {}
        for row in rows:
            path = row.get("file_path") or ""
            entry = merged.get(path)
            if entry is None:
                content = str(row.get("content") or "")
                # Collapse whitespace. The raw chunk keeps the document's
                # own line breaks and indentation, and the UI renders this
                # as a clamped snippet — where a run of blank lines and
                # tabs counts against the line budget and leaves the last
                # visible row sliced through the middle.
                snippet = " ".join(content.split())
                merged[path] = {
                    "id": row.get("id", ""),
                    "file_path": path,
                    "content_preview": snippet[:200] + ("…" if len(snippet) > 200 else ""),
                    "chunk_count": 1,
                }
            else:
                entry["chunk_count"] += 1
        return list(merged.values())


class RAGService:
    def __init__(self, embedding_service: EmbeddingService, db_path: str):
        self.embedding_service = embedding_service
        self.vector_store = LanceDBVectorStore(db_path)

    def index_file(self, file_path: str, content: str):
        self.index_files([{"file_path": file_path, "content": content}])

    def index_files(self, files: list[dict]):
        """Embed each file in chunks.

        One vector per whole document would both overflow the embedding model's
        context on large files and make every passage retrieve equally, so each
        chunk becomes its own vector tagged with its file.
        """
        records: list[dict] = []
        for f in files:
            # Delete old tails if a document was shortened or rechunked. The
            # deterministic merge updates matching ids, but cannot infer that
            # now-nonexistent chunk ids should be deleted.
            self.vector_store.delete_by_file(f["file_path"])
            for idx, (_offset, piece) in enumerate(chunk_file(f["content"])):
                records.append({
                    "file_path": f["file_path"],
                    "content": piece,
                    "chunk_idx": idx,
                })
        if not records:
            return

        # Bounded so a batch stays inside what the embedding endpoint tolerates.
        batch_size = EMBED_CHUNK_BATCH
        for i in range(0, len(records), batch_size):
            batch = records[i: i + batch_size]
            vectors = self.embedding_service.embed([r["content"] for r in batch])
            self.vector_store.add_many(vectors, batch)

    def _retrieve_for_answer(self, question: str, top_k: int, embedder=None) -> tuple[list[dict], str]:
        """Retrieve passages and build the grounding prompt.

        Split out from query() so the streaming variant can emit tokens without
        duplicating the retrieval and prompt assembly.
        """
        # Embeds the question and calls the LLM: same interaction path as chat.
        interaction.mark_active()
        emb = embedder or self.embedding_service
        query_vector = emb.embed([question])[0]
        results = self.vector_store.search(query_vector, top_k=top_k)

        context_parts = []
        for i, r in enumerate(results, 1):
            context_parts.append(f"【文档{i}】({r.get('file_path', 'unknown')}):\n{r.get('content', '')[:500]}")
        return results, "\n\n".join(context_parts)

    def _answer_messages(self, question: str, context: str) -> list[dict]:
        prompt = f"""基于以下文档内容回答问题。如果文档中没有相关信息，请说明无法回答。

问题: {question}

文档内容:
{context}

回答:"""
        return [
            {"role": "system", "content": "你是一个基于文档的问答助手。请根据提供的文档内容回答问题。"},
            {"role": "user", "content": prompt},
        ]

    def query(self, question: str, top_k: int = 3, embedder=None) -> dict:
        results, context = self._retrieve_for_answer(question, top_k, embedder=embedder)

        if not results:
            return {"answer": "没有找到相关文件来回答这个问题。", "sources": []}

        from .llm_client import chat_completion
        # RAG answer also uses the main provider — like digest, it's
        # an automated path with no user-facing provider selection.
        provider = state.get_chat_history().get_main_provider()
        if provider is None:
            return {"answer": "没有可用的 LLM provider；请到 /settings 添加。", "sources": []}
        result = chat_completion(
            messages=self._answer_messages(question, context),
            temperature=0.7,
            provider=provider,
        )
        answer = result.get("content", "")

        return {
            "answer": answer,
            "sources": [{"file_path": r.get("file_path", ""), "score": r.get("score", 0)} for r in results]
        }

    def clear_index(self):
        self.vector_store.clear()


class ChatHistoryService:
    def __init__(self, db_path: str):
        self.db_path = db_path
        self._lock = threading.Lock()
        self._init_db()

    def _get_conn(self) -> sqlite3.Connection:
        return db.connect(self.db_path)

    def _init_db(self):
        with self._lock:
            conn = self._get_conn()
            conn.execute("""
                CREATE TABLE IF NOT EXISTS llm_providers (
                    id TEXT PRIMARY KEY,
                    name TEXT NOT NULL,
                    base_url TEXT NOT NULL,
                    model TEXT NOT NULL,
                    api_key TEXT DEFAULT '',
                    is_main INTEGER DEFAULT 0,
                    max_tokens INTEGER,
                    reasoning_budget INTEGER NOT NULL DEFAULT 1024,
                    created_at INTEGER,
                    updated_at INTEGER
                )
            """)
            # Migrate existing rows: add max_tokens column if missing
            cols = [r[1] for r in conn.execute("PRAGMA table_info(llm_providers)")]
            if "max_tokens" not in cols:
                conn.execute("ALTER TABLE llm_providers ADD COLUMN max_tokens INTEGER")
            # reasoning_budget: the per-provider cap on the model's
            # thinking. Nullable, so a provider that never set one falls
            # back to the system default.
            if "reasoning_budget" not in cols:
                conn.execute(
                    "ALTER TABLE llm_providers ADD COLUMN reasoning_budget"
                    " INTEGER NOT NULL DEFAULT 1024"
                )
            else:
                # The column was added while the budget was still a
                # global default that a provider could decline to
                # carry. It is per-provider now, so the rows that never
                # set one get the default value.
                conn.execute(
                    "UPDATE llm_providers SET reasoning_budget = ?"
                    " WHERE reasoning_budget IS NULL",
                    (DEFAULT_REASONING_BUDGET,),
                )
            conn.execute("""
                CREATE TABLE IF NOT EXISTS chat_sessions (
                    id TEXT PRIMARY KEY,
                    title TEXT DEFAULT '',
                    session_type TEXT DEFAULT 'chat',
                    updated_at INTEGER,
                    provider_id TEXT,
                    -- Whose conversation this is. A session used to belong
                    -- to whoever happened to ask, which on a machine with
                    -- more than one person meant everybody could read
                    -- everybody's questions.
                    owner TEXT DEFAULT '',
                    FOREIGN KEY (provider_id) REFERENCES llm_providers(id) ON DELETE SET NULL
                )
            """)
            # A table that already exists does not get the new column
            # from CREATE TABLE, so it is added here.
            columns = {r[1] for r in conn.execute("PRAGMA table_info(chat_sessions)")}
            if "owner" not in columns:
                conn.execute("ALTER TABLE chat_sessions ADD COLUMN owner TEXT DEFAULT ''")
            conn.execute("""
                CREATE TABLE IF NOT EXISTS chat_messages (
                    id TEXT PRIMARY KEY,
                    session_id TEXT,
                    role TEXT,
                    content TEXT,
                    reasoning TEXT DEFAULT '',
                    activity TEXT DEFAULT '[]',
                    sources TEXT,
                    timestamp INTEGER,
                    ttft_ms INTEGER DEFAULT 0,
                    total_ms INTEGER DEFAULT 0,
                    tokens INTEGER DEFAULT 0,
                    tps REAL DEFAULT 0.0,
                    use_fast INTEGER DEFAULT 0,
                    provider_id TEXT,
                    FOREIGN KEY (session_id) REFERENCES chat_sessions(id) ON DELETE CASCADE,
                    FOREIGN KEY (provider_id) REFERENCES llm_providers(id) ON DELETE SET NULL
                )
            """)
            cols = [r[1] for r in conn.execute("PRAGMA table_info(chat_messages)")]
            if "reasoning" not in cols:
                conn.execute("ALTER TABLE chat_messages ADD COLUMN reasoning TEXT DEFAULT ''")
            if "activity" not in cols:
                conn.execute("ALTER TABLE chat_messages ADD COLUMN activity TEXT DEFAULT '[]'")
            # Per-turn perf columns. Older deployments saved without them; the
            # ALTER TABLE adds them on first init after the schema bump. The
            # defaults keep the SELECT contract safe for legacy rows.
            for col, decl in (
                ("ttft_ms", "INTEGER DEFAULT 0"),
                ("total_ms", "INTEGER DEFAULT 0"),
                ("tokens", "INTEGER DEFAULT 0"),
                ("tps", "REAL DEFAULT 0.0"),
                # use_fast: 0/1 in SQLite (no native bool). Added on the
                # same migration as the perf columns so legacy rows show
                # the regular 24B badge by default.
                ("use_fast", "INTEGER DEFAULT 0"),
                # provider_id: FK to llm_providers. Nullable for legacy
                # rows; the agent router backfills it from the main
                # provider at read time when it's missing.
                ("provider_id", "TEXT"),
                # plans: JSON array of AgentPlan dicts, populated when
                # the agent's submit_plan tool ran during this turn.
                # Drives the inline approve button in the chat bubble.
                # Stored as JSON rather than FKs because plans are
                # also their own first-class rows in the plans table;
                # this is just a snapshot for replay on reload.
                ("plans", "TEXT"),
            ):
                if col not in cols:
                    conn.execute(f"ALTER TABLE chat_messages ADD COLUMN {col} {decl}")
            sess_cols = [r[1] for r in conn.execute("PRAGMA table_info(chat_sessions)")]
            if "provider_id" not in sess_cols:
                conn.execute("ALTER TABLE chat_sessions ADD COLUMN provider_id TEXT")
            conn.execute("CREATE INDEX IF NOT EXISTS idx_messages_session ON chat_messages(session_id)")
            conn.execute("CREATE INDEX IF NOT EXISTS idx_providers_main ON llm_providers(is_main)")
            # Embedding providers live alongside LLM providers for
            # symmetry — same SQLite file, same locking, same singleton
            # enforcement. Two singletons (is_index, is_search) hang
            # off this table.
            conn.execute("""
                CREATE TABLE IF NOT EXISTS embedding_providers (
                    id TEXT PRIMARY KEY,
                    name TEXT NOT NULL,
                    base_url TEXT NOT NULL,
                    model TEXT NOT NULL,
                    dim TEXT DEFAULT 'AUTO',
                    api_key TEXT DEFAULT '',
                    is_index INTEGER DEFAULT 0,
                    is_search INTEGER DEFAULT 0,
                    created_at INTEGER,
                    updated_at INTEGER
                )
            """)
            conn.execute("CREATE INDEX IF NOT EXISTS idx_emb_providers_index ON embedding_providers(is_index)")
            conn.execute("CREATE INDEX IF NOT EXISTS idx_emb_providers_search ON embedding_providers(is_search)")
            conn.commit()
            conn.close()

    # ---- LLM provider CRUD ----------------------------------------------
    #
    # The providers table owns the LLM endpoints the agent flow talks
    # to. Methods below are the only place that should be writing to
    # `llm_providers`; everything else goes through the API router.
    # The `is_main` flag is enforced as a singleton: setting it on
    # one row unmarks the rest in the same transaction so a query
    # like "WHERE is_main=1" can rely on returning at most one row.

    def list_providers(self) -> list[LLMProvider]:
        with self._lock:
            conn = self._get_conn()
            # Sort by creation order only. The is_main flag is
            # surfaced via the MAIN badge on the row, so reordering
            # when main flips would only move an already-marked
            # row — the user expects "set as main" to flip the
            # badge, not to jump the row to the top.
            rows = conn.execute(
                "SELECT id, name, base_url, model, api_key, is_main, max_tokens,"
                " reasoning_budget, created_at, updated_at"
                " FROM llm_providers ORDER BY created_at ASC"
            ).fetchall()
            conn.close()
        return [self._row_to_provider(r) for r in rows]

    def get_provider(self, provider_id: str) -> Optional[LLMProvider]:
        with self._lock:
            conn = self._get_conn()
            row = conn.execute(
                "SELECT id, name, base_url, model, api_key, is_main, max_tokens,"
                " reasoning_budget, created_at, updated_at"
                " FROM llm_providers WHERE id = ?",
                (provider_id,),
            ).fetchone()
            conn.close()
        return self._row_to_provider(row) if row else None

    def get_main_provider(self) -> Optional[LLMProvider]:
        with self._lock:
            conn = self._get_conn()
            row = conn.execute(
                "SELECT id, name, base_url, model, api_key, is_main, max_tokens,"
                " reasoning_budget, created_at, updated_at"
                " FROM llm_providers WHERE is_main = 1 LIMIT 1"
            ).fetchone()
            conn.close()
        return self._row_to_provider(row) if row else None

    def create_provider(self, name: str, base_url: str, model: str,
                        api_key: str = "",
                        max_tokens: Optional[int] = None,
                        reasoning_budget: Optional[int] = None) -> LLMProvider:
        provider_id = uuid.uuid4().hex
        now = int(time.time() * 1000)
        with self._lock:
            conn = self._get_conn()
            # If this is the very first provider the system has seen,
            # promote it to main automatically — new sessions need a
            # default to land on.
            existing_main = conn.execute(
                "SELECT 1 FROM llm_providers WHERE is_main = 1 LIMIT 1"
            ).fetchone()
            is_main = 0 if existing_main else 1
            conn.execute(
                "INSERT INTO llm_providers (id, name, base_url, model, api_key,"
                " is_main, max_tokens, reasoning_budget, created_at, updated_at)"
                " VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                (provider_id, name, base_url, model, api_key,
                 is_main, max_tokens, reasoning_budget, now, now)
            )
            conn.commit()
            conn.close()
        return LLMProvider(
            id=provider_id, name=name, base_url=base_url, model=model,
            api_key=api_key, is_main=bool(is_main),
            max_tokens=max_tokens,
            reasoning_budget=(
                reasoning_budget if reasoning_budget is not None
                else DEFAULT_REASONING_BUDGET
            ),
            created_at=now, updated_at=now
        )

    def update_provider(self, provider_id: str, *,
                         name: Optional[str] = None,
                         base_url: Optional[str] = None,
                         model: Optional[str] = None,
                         api_key: Optional[str] = None,
                         is_main: Optional[bool] = None,
                         max_tokens: Optional[int] = None,
                         reasoning_budget: Any = _UNSET
                         ) -> Optional[LLMProvider]:
        with self._lock:
            conn = self._get_conn()
            existing = conn.execute(
                "SELECT id FROM llm_providers WHERE id = ?", (provider_id,)
            ).fetchone()
            if not existing:
                conn.close()
                return None
            fields: list[str] = []
            values: list = []
            if name is not None:
                fields.append("name = ?"); values.append(name)
            if base_url is not None:
                fields.append("base_url = ?"); values.append(base_url)
            if model is not None:
                fields.append("model = ?"); values.append(model)
            if api_key is not None:
                fields.append("api_key = ?"); values.append(api_key)
            if max_tokens is not None:
                fields.append("max_tokens = ?"); values.append(max_tokens)
            if reasoning_budget is not _UNSET:
                # Distinct from "leave it alone": the operator can put a
                # provider back on the system default by clearing the
                # field, which is a null rather than an omission.
                fields.append("reasoning_budget = ?"); values.append(reasoning_budget)
            if is_main is True:
                # Promote this row and demote everyone else. Both halves
                # run in the same lock so an external query between the
                # two statements can't see two main providers.
                conn.execute("UPDATE llm_providers SET is_main = 0")
                fields.append("is_main = ?"); values.append(1)
            elif is_main is False:
                # Caller is asking to demote this provider. The route
                # layer is responsible for ensuring a main remains
                # (refuses if this is the only provider).
                fields.append("is_main = ?"); values.append(0)
            if fields:
                fields.append("updated_at = ?")
                values.append(int(time.time() * 1000))
                values.append(provider_id)
                conn.execute(
                    f"UPDATE llm_providers SET {', '.join(fields)} WHERE id = ?",
                    values
                )
                conn.commit()
            conn.close()
        return self.get_provider(provider_id)

    def delete_provider(self, provider_id: str) -> bool:
        # The router layer guards against deleting the main; sessions
        # referencing this provider will have their provider_id set to
        # NULL via ON DELETE SET NULL and the agent flow will backfill
        # from the main at read time.
        with self._lock:
            conn = self._get_conn()
            cur = conn.execute(
                "DELETE FROM llm_providers WHERE id = ?", (provider_id,)
            )
            conn.commit()
            deleted = cur.rowcount > 0
            conn.close()
            return deleted

    def _row_to_provider(self, row) -> LLMProvider:
        (pid, name, base_url, model, api_key, is_main, max_tokens,
         reasoning_budget, created_at, updated_at) = row
        return LLMProvider(
            id=pid, name=name, base_url=base_url, model=model,
            api_key=api_key or "", is_main=bool(is_main),
            max_tokens=max_tokens or None,
            reasoning_budget=(
                reasoning_budget if reasoning_budget is not None
                else DEFAULT_REASONING_BUDGET
            ),
            created_at=created_at or 0, updated_at=updated_at or 0
        )

    def create_session(self, session_type: str = "chat", owner: str = "") -> ChatSession:
        with self._lock:
            session_id = uuid.uuid4().hex
            now = int(time.time() * 1000)
            conn = self._get_conn()
            # New sessions inherit the main provider by default. The
            # user can switch via the chat UI, which writes a different
            # provider_id back into this row.
            main = conn.execute(
                "SELECT id FROM llm_providers WHERE is_main = 1 LIMIT 1"
            ).fetchone()
            main_id = main[0] if main else None
            conn.execute(
                "INSERT INTO chat_sessions (id, title, session_type, updated_at, provider_id, owner)"
                " VALUES (?, ?, ?, ?, ?, ?)",
                (session_id, "", session_type, now, main_id, owner or "")
            )
            conn.commit()
            conn.close()
        return ChatSession(id=session_id, title="", messages=[], updated_at=now,
                          session_type=session_type, provider_id=main_id)

    def get_sessions(self, session_type: Optional[str] = None, owner: Optional[str] = None) -> list[ChatSession]:
        """The conversations, newest first.

        `owner` is who is asking. A person sees their own; the
        administrator sees everybody's. `owner` of None means everybody.
        """
        with self._lock:
            conn = self._get_conn()
            where = []
            params: list = []
            if session_type:
                where.append("session_type = ?")
                params.append(session_type)
            clause = (" WHERE " + " AND ".join(where)) if where else ""
            rows = conn.execute(
                "SELECT id, title, session_type, updated_at, provider_id, owner"
                f" FROM chat_sessions{clause} ORDER BY updated_at DESC",
                tuple(params),
            ).fetchall()

            sessions = []
            for row in rows:
                session_id, title, sess_type, updated_at, provider_id, owner = row
                # A conversation belongs to the person who started it.
                if not _session_visible_to(owner):
                    continue
                messages = self._get_messages(conn, session_id)
                sessions.append(ChatSession(
                    id=session_id,
                    title=title or "",
                    messages=messages,
                    updated_at=updated_at,
                    session_type=sess_type,
                    provider_id=provider_id,
                ))
            conn.close()
        return sessions

    def _get_messages(self, conn: sqlite3.Connection, session_id: str) -> list[ChatMessage]:
        rows = conn.execute(
            "SELECT id, role, content, reasoning, activity, sources, timestamp,"
            " ttft_ms, total_ms, tokens, tps, use_fast, provider_id, plans"
            " FROM chat_messages WHERE session_id = ? ORDER BY timestamp ASC",
            (session_id,)
        ).fetchall()
        messages = []
        for row in rows:
            (msg_id, role, content, reasoning, activity, sources, timestamp,
             ttft_ms, total_ms, tokens, tps, use_fast, provider_id, plans) = row
            sources_list = json.loads(sources) if sources else None
            try:
                activity_list = json.loads(activity) if activity else []
            except (TypeError, ValueError):
                activity_list = []
            # plans was added later than the rest of the perf columns;
            # NULL on legacy rows. Empty string vs NULL is an
            # implementation detail of how sqlite reports it, so guard
            # both. A malformed JSON blob (corrupt db, partial write)
            # is treated as no plans rather than crashing the read.
            plans_list = None
            if plans:
                try:
                    plans_list = json.loads(plans)
                except (TypeError, ValueError):
                    plans_list = None
            messages.append(ChatMessage(
                id=msg_id,
                role=role,
                reasoning=reasoning or "",
                activity=activity_list,
                content=content,
                sources=sources_list,
                timestamp=timestamp,
                ttft_ms=ttft_ms or 0,
                total_ms=total_ms or 0,
                tokens=tokens or 0,
                tps=tps or 0.0,
                # Legacy `use_fast` rows have provider_id NULL. The
                # chat UI never reads use_fast anymore; the persisted
                # bubble shows the resolved provider name instead.
                use_fast=bool(use_fast),
                provider_id=provider_id,
                plans=plans_list,
            ))
        return messages

    def get_session(self, session_id: str) -> Optional[ChatSession]:
        """One conversation, and everything said in it.

        `owner` is who is asking: a person may open their own, and the
        administrator may open anybody's.
        """
        with self._lock:
            conn = self._get_conn()
            row = conn.execute(
                "SELECT id, title, session_type, updated_at, owner FROM chat_sessions WHERE id = ?",
                (session_id,)
            ).fetchone()
            if not row:
                conn.close()
                return None
            session_id, title, sess_type, updated_at, owner = row
            if not _session_visible_to(owner):
                conn.close()
                return None
            messages = self._get_messages(conn, session_id)
            conn.close()
        return ChatSession(
            id=session_id,
            title=title or "",
            messages=messages,
            updated_at=updated_at,
            session_type=sess_type
        )

    def add_message(self, session_id: str, role: str, content: str, sources: Optional[list] = None,
                    reasoning: str = "", activity: Optional[list] = None,
                    ttft_ms: int = 0, total_ms: int = 0, tokens: int = 0, tps: float = 0.0,
                    provider_id: Optional[str] = None,
                    plans: Optional[list] = None) -> ChatMessage:
        msg_id = uuid.uuid4().hex
        now = int(time.time() * 1000)
        sources_json = json.dumps(sources) if sources else None
        plans_json = json.dumps(plans, ensure_ascii=False) if plans else None
        with self._lock:
            conn = self._get_conn()
            # use_fast is no longer written — the column stays for
            # back-compat with rows saved before the providers
            # refactor, but new rows just take the column default (0).
            conn.execute(
                "INSERT INTO chat_messages (id, session_id, role, content, reasoning, activity,"
                " sources, plans, timestamp, ttft_ms, total_ms, tokens, tps, provider_id)"
                " VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                (msg_id, session_id, role, content, reasoning,
                 json.dumps(activity, ensure_ascii=False) if activity else "[]", sources_json,
                 plans_json, now,
                 ttft_ms, total_ms, tokens, tps, provider_id)
            )
            conn.execute(
                "UPDATE chat_sessions SET updated_at = ? WHERE id = ?",
                (now, session_id)
            )
            conn.commit()
            conn.close()
        return ChatMessage(
            id=msg_id, role=role, content=content, reasoning=reasoning,
            activity=activity or [], sources=sources, timestamp=now,
            ttft_ms=ttft_ms, total_ms=total_ms, tokens=tokens, tps=tps,
            provider_id=provider_id, plans=plans,
        )

    def delete_session(self, session_id: str):
        with self._lock:
            conn = self._get_conn()
            conn.execute("DELETE FROM chat_messages WHERE session_id = ?", (session_id,))
            conn.execute("DELETE FROM chat_sessions WHERE id = ?", (session_id,))
            conn.commit()
            conn.close()

    def update_session_title(self, session_id: str, title: str):
        with self._lock:
            conn = self._get_conn()
            conn.execute("UPDATE chat_sessions SET title = ? WHERE id = ?", (title, session_id))
            conn.commit()
            conn.close()

    def update_session_provider(self, session_id: str, provider_id: str):
        """Sticky the per-session provider the user picked in the chat UI.

        Called by the agent router after a per-turn override is applied,
        so subsequent messages in the same conversation keep using the
        same provider. Takes the lock so concurrent reads see a coherent
        row state.
        """
        with self._lock:
            conn = self._get_conn()
            conn.execute(
                "UPDATE chat_sessions SET provider_id = ? WHERE id = ?",
                (provider_id, session_id),
            )
            conn.commit()
            conn.close()

    # ---- Embedding provider CRUD ----------------------------------------
    #
    # Two singletons hang off this table: is_index (the indexer reads
    # this row) and is_search (the search path reads this row, falling
    # back to the index row when unset). The create method auto-promotes
    # the first row to is_index so an empty table never strands the
    # system. is_search is genuinely optional and may be empty.
    #
    # The dim field is the operator-declared embedding dimension. "AUTO"
    # is resolved to a concrete integer on first use; once resolved, the
    # indexer enforces the dim against the LanceDB schema so a
    # misconfiguration is loud rather than silent. Swapping the index
    # row to a different-dim provider does NOT auto-reindex — the
    # operator must drop the vector table by hand (or trigger a
    # reindex via the existing indexer run paths). This matches how
    # LanceDB itself models schema changes.

    def list_embedding_providers(self) -> list[EmbeddingProvider]:
        with self._lock:
            conn = self._get_conn()
            # Sort by creation order only. The roles (is_index /
            # is_search) are surfaced via badges on each row, so
            # reordering the list when a role flips would only
            # move the visual position of an already-marked row,
            # which is jarring — the user expects "set as index"
            # to flip a badge, not to jump the row to the top.
            rows = conn.execute(
                "SELECT id, name, base_url, model, dim, api_key,"
                " is_index, is_search, created_at, updated_at"
                " FROM embedding_providers ORDER BY created_at ASC"
            ).fetchall()
            conn.close()
        return [self._row_to_embedding_provider(r) for r in rows]

    def get_embedding_provider(self, provider_id: str) -> Optional[EmbeddingProvider]:
        with self._lock:
            conn = self._get_conn()
            row = conn.execute(
                "SELECT id, name, base_url, model, dim, api_key,"
                " is_index, is_search, created_at, updated_at"
                " FROM embedding_providers WHERE id = ?",
                (provider_id,),
            ).fetchone()
            conn.close()
        return self._row_to_embedding_provider(row) if row else None

    def get_index_embedding_provider(self) -> Optional[EmbeddingProvider]:
        with self._lock:
            conn = self._get_conn()
            row = conn.execute(
                "SELECT id, name, base_url, model, dim, api_key,"
                " is_index, is_search, created_at, updated_at"
                " FROM embedding_providers WHERE is_index = 1 LIMIT 1"
            ).fetchone()
            conn.close()
        return self._row_to_embedding_provider(row) if row else None

    def get_search_embedding_provider(self) -> Optional[EmbeddingProvider]:
        """Search path's provider. Prefers is_search=1; falls back to
        the is_index row when search is unset (cloud-only deployments
        share one endpoint)."""
        with self._lock:
            conn = self._get_conn()
            row = conn.execute(
                "SELECT id, name, base_url, model, dim, api_key,"
                " is_index, is_search, created_at, updated_at"
                " FROM embedding_providers WHERE is_search = 1 LIMIT 1"
            ).fetchone()
            if not row:
                row = conn.execute(
                    "SELECT id, name, base_url, model, dim, api_key,"
                    " is_index, is_search, created_at, updated_at"
                    " FROM embedding_providers WHERE is_index = 1 LIMIT 1"
                ).fetchone()
            conn.close()
        return self._row_to_embedding_provider(row) if row else None

    def create_embedding_provider(
        self, name: str, base_url: str, model: str,
        dim: str = "AUTO", api_key: str = "",
    ) -> EmbeddingProvider:
        provider_id = uuid.uuid4().hex
        now = int(time.time() * 1000)
        with self._lock:
            conn = self._get_conn()
            # First provider auto-promotes to is_index. is_search is
            # opt-in via PATCH so the user explicitly chooses whether
            # to dedicate this endpoint to the search path.
            existing_index = conn.execute(
                "SELECT 1 FROM embedding_providers WHERE is_index = 1 LIMIT 1"
            ).fetchone()
            is_index = 0 if existing_index else 1
            conn.execute(
                "INSERT INTO embedding_providers (id, name, base_url, model, dim,"
                " api_key, is_index, is_search, created_at, updated_at)"
                " VALUES (?, ?, ?, ?, ?, ?, ?, 0, ?, ?)",
                (provider_id, name, base_url, model, dim, api_key,
                 is_index, now, now)
            )
            conn.commit()
            conn.close()
        return EmbeddingProvider(
            id=provider_id, name=name, base_url=base_url, model=model,
            dim=dim, api_key=api_key, is_index=bool(is_index),
            is_search=False, created_at=now, updated_at=now,
        )

    def update_embedding_provider(self, provider_id: str, *,
                                  name: Optional[str] = None,
                                  base_url: Optional[str] = None,
                                  model: Optional[str] = None,
                                  dim: Optional[str] = None,
                                  api_key: Optional[str] = None,
                                  is_index: Optional[bool] = None,
                                  is_search: Optional[bool] = None) -> Optional[EmbeddingProvider]:
        with self._lock:
            conn = self._get_conn()
            existing = conn.execute(
                "SELECT id FROM embedding_providers WHERE id = ?", (provider_id,)
            ).fetchone()
            if not existing:
                conn.close()
                return None
            fields: list[str] = []
            values: list = []
            if name is not None:
                fields.append("name = ?"); values.append(name)
            if base_url is not None:
                fields.append("base_url = ?"); values.append(base_url)
            if model is not None:
                fields.append("model = ?"); values.append(model)
            if dim is not None:
                fields.append("dim = ?"); values.append(dim)
            if api_key is not None:
                fields.append("api_key = ?"); values.append(api_key)
            if is_index is True:
                # Singleton promote: demote the rest before stamping
                # this row. Both halves run in the same lock so an
                # external query between the two statements cannot see
                # two index providers.
                conn.execute("UPDATE embedding_providers SET is_index = 0")
                fields.append("is_index = ?"); values.append(1)
            elif is_index is False:
                # Caller is asking to demote this row. The router
                # layer is responsible for ensuring another is_index
                # row remains (refuses when this is the only one).
                fields.append("is_index = ?"); values.append(0)
            if is_search is True:
                # Search is also a singleton. Demote the rest so
                # "search is exactly one row" stays an invariant.
                conn.execute("UPDATE embedding_providers SET is_search = 0")
                fields.append("is_search = ?"); values.append(1)
            elif is_search is False:
                # No "last search" rule — search may be empty (the
                # system falls back to the index row).
                fields.append("is_search = ?"); values.append(0)
            if fields:
                fields.append("updated_at = ?")
                values.append(int(time.time() * 1000))
                values.append(provider_id)
                conn.execute(
                    f"UPDATE embedding_providers SET {', '.join(fields)} WHERE id = ?",
                    values
                )
                conn.commit()
            conn.close()
        return self.get_embedding_provider(provider_id)

    def delete_embedding_provider(self, provider_id: str) -> bool:
        with self._lock:
            conn = self._get_conn()
            # Refuse silently: router layer is responsible for the
            # "is_index" guard since it knows whether this is the
            # last index row.
            cur = conn.execute(
                "DELETE FROM embedding_providers WHERE id = ?", (provider_id,)
            )
            conn.commit()
            deleted = cur.rowcount > 0
            conn.close()
            return deleted

    def _row_to_embedding_provider(self, row) -> EmbeddingProvider:
        if row is None:
            return None  # type: ignore[return-value]
        (pid, name, base_url, model, dim, api_key,
         is_index, is_search, created_at, updated_at) = row
        return EmbeddingProvider(
            id=pid, name=name, base_url=base_url, model=model,
            dim=dim or "AUTO", api_key=api_key or "",
            is_index=bool(is_index), is_search=bool(is_search),
            created_at=created_at or 0, updated_at=updated_at or 0,
        )


def get_storage_root() -> Path:
    """Resolve the user-facing storage root from current settings (never the .simplefilemanager dir)."""
    return (BASE_DIR / state.get_settings().storage_path).resolve()


class PathNotAllowed(Exception):
    """The caller may not reach the path they asked for."""


class PathOutsideRoot(Exception):
    pass


def resolve_under_root(path: str, mode: str = "read") -> Path:
    """Resolve a supplied path and guarantee it stays inside the storage root.

    Also checks that the caller may reach it. Every route that touches a
    file goes through here, so this is where a permission is enforced
    rather than in each route that might forget.

    `mode` is "read" or "write". A write grant implies a read one.
    """
    root = get_storage_root()
    clean = (path or "").strip()
    if clean in ("", "."):
        target = root
    else:
        target = (root / clean).resolve()
        if not target.is_relative_to(root):
            raise PathOutsideRoot(path)
    check_access(target, mode)
    return target


# The user the current request is being served for. Set by the auth
# dependency, read by the access checks.
_current_user: ContextVar[Optional[dict]] = ContextVar("current_user", default=None)


def set_current_user(user: Optional[dict]):
    """Remember who is being served, for the access checks."""
    _current_user.set(user)


def current_user() -> Optional[dict]:
    """The user being served, or None when the service is open."""
    return _current_user.get()


def _plan_visible_to(owner: str) -> bool:
    """Whether the person asking may see this plan.

    A plan moves real files. It is the person's, and only the
    administrator may approve somebody else's.
    """
    user = current_user()
    if user is None:
        # No accounts: the service is open, and so are the plans.
        return True
    if user.get("role") == "admin":
        return True
    if not owner:
        # A plan from before there were accounts belongs to the
        # administrator, which is who the single password was.
        return False
    return owner == str(user.get("id") or "") or owner == str(user.get("username") or "")


def session_owner() -> str:
    """Who the current request belongs to, for recording on a session."""
    user = current_user()
    if user is None:
        return ""
    return str(user.get("id") or user.get("username") or "")


def _session_visible_to(owner: str) -> bool:
    """Whether the person asking may see this conversation.

    A conversation belongs to the person who started it. The
    administrator may see anybody's, and nobody else may see anybody's
    but their own. When no accounts exist the service is open, and the
    conversations are too.
    """
    user = current_user()
    if user is None:
        return True
    if user.get("role") == "admin":
        return True
    if not owner:
        # A conversation from before there were accounts belongs to
        # the administrator, which is who the single password was.
        return False
    return owner == str(user.get("id") or "") or owner == str(user.get("username") or "")


def effective_restriction(
    relative_path: str, restrictions: list[tuple[str, str]]
) -> Optional[str]:
    """The exception that applies to a path, if there is one.

    A restriction on a directory covers everything under it, and the
    most specific one for a path is the one that counts -- so a
    restriction on "photos/2024" beats one on "photos". No restriction
    at all means the path is open, which is how a machine a family
    shares is meant to work.
    """
    rel = (relative_path or "").strip().strip("/")
    best = None
    best_depth = -1
    for restricted_path, restriction in restrictions:
        g = (restricted_path or "").strip().strip("/")
        if g == "":
            matches = True
        else:
            matches = rel == g or rel.startswith(g + "/")
        if not matches:
            continue
        depth = len(g)
        if depth >= best_depth:
            best = restriction
            best_depth = depth
    return best


def check_access(target: Path, mode: str = "read") -> None:
    """Refuse a path the caller may not reach.

    This used to be a list of grants, so a path was shut until
    somebody was given the key. On a machine a family shares that is
    the wrong way round: everybody has a key to the same house, and a
    folder is private only when somebody says so. So the list is a
    list of exceptions, and a path nobody has been shut out of is
    open to everybody.

    The administrator may reach everything, and so may anyone when
    the service has no users set up at all -- a single-password
    installation must keep working the way it always did.
    """
    user = current_user()
    if user is None:
        return
    if user.get("role") == "admin":
        return

    root = get_storage_root().resolve()
    try:
        relative = str(Path(target).resolve().relative_to(root))
    except ValueError:
        raise PathNotAllowed("没有访问这个位置的权限")

    store = state.get_user_store()
    restriction = effective_restriction(
        relative, store.restrictions_for(user["id"])
    )
    if restriction is None:
        return
    if restriction == "none":
        raise PathNotAllowed("没有访问这个位置的权限")
    if mode == "write":
        raise PathNotAllowed("这个位置是只读的")


def can_access(target: Path, mode: str = "read") -> bool:
    """Whether the caller may reach a path. Never raises."""
    try:
        check_access(target, mode)
        return True
    except PathNotAllowed:
        return False


def filter_accessible(paths: list, mode: str = "read") -> list:
    """Keep only the paths the caller may reach.

    Search results and directory listings are produced from the whole
    corpus, and the corpus holds everything -- including what one
    person may not open. So they are filtered here, at the one place
    that knows who is asking.
    """
    return [p for p in paths if can_access(Path(p), mode)]


def visible_children(target: Path, entries: list[Path]) -> list[Path]:
    """The entries of a directory the caller is allowed to see.

    A user granted access to one folder should not see the names of
    the others in the folder above it.
    """
    user = current_user()
    if user is None or user.get("role") == "admin":
        return entries
    return [e for e in entries if can_access(e, "read")]


def validate_plan_action(action: dict) -> Optional[str]:
    """Return an error message if a plan action is malformed or escapes the storage root, else None."""
    action_type = action.get("action_type", "")
    if action_type not in ("move", "rename", "create_folder", "delete", "run"):
        return f"Unknown action_type: {action_type}"
    if action_type == "run":
        command = (action.get("command") or "").strip()
        if not command:
            return "run requires command"
        if len(command) > 8000:
            return "run command is too long (max 8000 characters)"
        cwd = (action.get("cwd") or "").strip()
        if cwd.startswith("/") or cwd.startswith("~"):
            return "run cwd must be relative to the storage root"
        return None
    if action_type in ("move", "rename", "delete") and not (action.get("source_path") or "").strip():
        return f"{action_type} requires source_path"
    if action_type in ("move", "rename", "create_folder") and not (action.get("target_path") or "").strip():
        return f"{action_type} requires target_path"
    try:
        if action_type in ("move", "rename", "delete"):
            resolve_under_root(action["source_path"])
        if action_type in ("move", "rename", "create_folder"):
            resolve_under_root(action["target_path"])
    except PathOutsideRoot as e:
        return f"Path outside storage root: {e}"
    return None


class PlanStore:
    def __init__(self, db_path: str):
        self.db_path = db_path
        self._lock = threading.Lock()
        self._init_db()

    def _get_conn(self) -> sqlite3.Connection:
        return db.connect(self.db_path)

    def _init_db(self):
        with self._lock:
            conn = self._get_conn()
            conn.execute("""
                CREATE TABLE IF NOT EXISTS plans (
                    id TEXT PRIMARY KEY,
                    title TEXT NOT NULL,
                    summary TEXT DEFAULT '',
                    status TEXT DEFAULT 'pending',
                    source TEXT DEFAULT 'agent',
                    created_at INTEGER,
                    decided_at INTEGER,
                    executed_at INTEGER,
                    -- Whose plan this is. A plan moves real files, so it
                    -- is the person's, and only the administrator may
                    -- approve somebody else's.
                    owner TEXT DEFAULT ''
                )
            """)
            plan_cols = {r[1] for r in conn.execute("PRAGMA table_info(plans)")}
            if "owner" not in plan_cols:
                conn.execute("ALTER TABLE plans ADD COLUMN owner TEXT DEFAULT ''")
            conn.execute("""
                CREATE TABLE IF NOT EXISTS plan_actions (
                    id TEXT PRIMARY KEY,
                    plan_id TEXT NOT NULL,
                    action_type TEXT NOT NULL,
                    source_path TEXT,
                    target_path TEXT,
                    reason TEXT DEFAULT '',
                    status TEXT DEFAULT 'pending',
                    result TEXT,
                    position INTEGER DEFAULT 0,
                    FOREIGN KEY (plan_id) REFERENCES plans(id) ON DELETE CASCADE
                )
            """)
            action_cols = {r[1] for r in conn.execute("PRAGMA table_info(plan_actions)")}
            if "command" not in action_cols:
                conn.execute("ALTER TABLE plan_actions ADD COLUMN command TEXT")
            if "cwd" not in action_cols:
                conn.execute("ALTER TABLE plan_actions ADD COLUMN cwd TEXT")
            conn.execute("""
                CREATE TABLE IF NOT EXISTS execution_log (
                    id TEXT PRIMARY KEY,
                    plan_id TEXT NOT NULL,
                    action_id TEXT,
                    action_type TEXT,
                    source_path TEXT,
                    target_path TEXT,
                    success INTEGER,
                    message TEXT,
                    timestamp INTEGER
                )
            """)
            conn.execute("CREATE INDEX IF NOT EXISTS idx_actions_plan ON plan_actions(plan_id)")
            conn.execute("CREATE INDEX IF NOT EXISTS idx_plans_status ON plans(status)")
            conn.commit()
            conn.close()

    def create_plan(self, title: str, summary: str, source: str, actions: list[dict], owner: str = "") -> AgentPlan:
        with self._lock:
            plan_id = uuid.uuid4().hex
            now = int(time.time() * 1000)
            conn = self._get_conn()
            conn.execute(
                "INSERT INTO plans (id, title, summary, status, source, created_at, owner) VALUES (?, ?, ?, 'pending', ?, ?, ?)",
                (plan_id, title, summary, source, now, owner or "")
            )
            action_models = []
            for pos, a in enumerate(actions):
                action_id = uuid.uuid4().hex
                conn.execute(
                    "INSERT INTO plan_actions (id, plan_id, action_type, source_path, target_path, command, cwd, reason, position) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
                    (action_id, plan_id, a.get("action_type", "move"), a.get("source_path"),
                     a.get("target_path"), a.get("command"), a.get("cwd"),
                     a.get("reason", ""), pos)
                )
                action_models.append(PlanAction(
                    id=action_id,
                    action_type=a.get("action_type", "move"),
                    source_path=a.get("source_path"),
                    target_path=a.get("target_path"),
                    command=a.get("command"),
                    cwd=a.get("cwd"),
                    reason=a.get("reason", ""),
                ))
            conn.commit()
            conn.close()
        return AgentPlan(
            id=plan_id, title=title, summary=summary, status="pending",
            source=source, actions=action_models, created_at=now,
        )

    def _row_to_plan(self, conn: sqlite3.Connection, row) -> AgentPlan:
        plan_id, title, summary, status, source, created_at, decided_at, executed_at, owner = row
        action_rows = conn.execute(
            "SELECT id, action_type, source_path, target_path, command, cwd, reason, status, result FROM plan_actions WHERE plan_id = ? ORDER BY position ASC",
            (plan_id,)
        ).fetchall()
        actions = [
            PlanAction(id=r[0], action_type=r[1], source_path=r[2], target_path=r[3],
                       command=r[4], cwd=r[5], reason=r[6] or "", status=r[7], result=r[8])
            for r in action_rows
        ]
        return AgentPlan(
            id=plan_id, title=title, summary=summary or "", status=status, source=source,
            actions=actions, created_at=created_at, decided_at=decided_at, executed_at=executed_at,
        )

    def list_plans(self, status: Optional[str] = None, limit: int = 50) -> list[AgentPlan]:
        with self._lock:
            conn = self._get_conn()
            if status:
                rows = conn.execute(
                    "SELECT id, title, summary, status, source, created_at, decided_at, executed_at, owner FROM plans WHERE status = ? ORDER BY created_at DESC LIMIT ?",
                    (status, limit)
                ).fetchall()
            else:
                rows = conn.execute(
                    "SELECT id, title, summary, status, source, created_at, decided_at, executed_at, owner FROM plans ORDER BY created_at DESC LIMIT ?",
                    (limit,)
                ).fetchall()
            plans = [
                self._row_to_plan(conn, r)
                for r in rows
                if _plan_visible_to(r[8])
            ]
            conn.close()
        return plans

    def get_plan(self, plan_id: str) -> Optional[AgentPlan]:
        with self._lock:
            conn = self._get_conn()
            row = conn.execute(
                "SELECT id, title, summary, status, source, created_at, decided_at, executed_at, owner FROM plans WHERE id = ?",
                (plan_id,)
            ).fetchone()
            if not row:
                conn.close()
                return None
            if not _plan_visible_to(row[8]):
                conn.close()
                return None
            plan = self._row_to_plan(conn, row)
            conn.close()
        return plan

    def update_status(self, plan_id: str, status: str, decided: bool = False, executed: bool = False) -> Optional[AgentPlan]:
        now = int(time.time() * 1000)
        with self._lock:
            conn = self._get_conn()
            sets, args = ["status = ?"], [status]
            if decided:
                sets.append("decided_at = ?")
                args.append(now)
            if executed:
                sets.append("executed_at = ?")
                args.append(now)
            args.append(plan_id)
            conn.execute(f"UPDATE plans SET {', '.join(sets)} WHERE id = ?", args)
            conn.commit()
            conn.close()
        return self.get_plan(plan_id)

    def update_action(self, action_id: str, status: str, result: Optional[str] = None):
        with self._lock:
            conn = self._get_conn()
            conn.execute(
                "UPDATE plan_actions SET status = ?, result = ? WHERE id = ?",
                (status, result, action_id)
            )
            conn.commit()
            conn.close()

    def add_log(self, plan_id: str, action_id: Optional[str], action_type: str,
                source_path: Optional[str], target_path: Optional[str], success: bool, message: str):
        with self._lock:
            conn = self._get_conn()
            conn.execute(
                "INSERT INTO execution_log (id, plan_id, action_id, action_type, source_path, target_path, success, message, timestamp) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
                (uuid.uuid4().hex, plan_id, action_id, action_type, source_path, target_path,
                 int(success), message, int(time.time() * 1000))
            )
            conn.commit()
            conn.close()

    def get_log(self, plan_id: str) -> list[dict]:
        with self._lock:
            conn = self._get_conn()
            rows = conn.execute(
                "SELECT id, action_id, action_type, source_path, target_path, success, message, timestamp FROM execution_log WHERE plan_id = ? ORDER BY timestamp ASC",
                (plan_id,)
            ).fetchall()
            conn.close()
        return [
            {"id": r[0], "action_id": r[1], "action_type": r[2], "source_path": r[3],
             "target_path": r[4], "success": bool(r[5]), "message": r[6], "timestamp": r[7]}
            for r in rows
        ]

    def delete_plan(self, plan_id: str):
        with self._lock:
            conn = self._get_conn()
            conn.execute("DELETE FROM plan_actions WHERE plan_id = ?", (plan_id,))
            conn.execute("DELETE FROM execution_log WHERE plan_id = ?", (plan_id,))
            conn.execute("DELETE FROM plans WHERE id = ?", (plan_id,))
            conn.commit()
            conn.close()


def _get_default_settings() -> AppSettings:
    return AppSettings(
        # LLM credentials live in the llm_providers table (see
        # _seed_providers_from_settings). Embedding credentials live
        # in the embedding_providers table (see
        # _seed_embedding_providers_from_settings). Both seeders read
        # the legacy env vars and settings.json fields, so leaving
        # them in .env keeps existing deployments upgrading in place
        # — no env rewrites required.
        index_interval=int(os.getenv("INDEX_INTERVAL", "300")),
        auto_index_enabled=os.getenv("AUTO_INDEX_ENABLED", "true").lower() in ("true", "1", "yes"),
        index_debounce_seconds=int(os.getenv("INDEX_DEBOUNCE_SECONDS", "10")),
        max_agent_steps=int(os.getenv("MAX_AGENT_STEPS", "8")),
        storage_path=os.getenv("STORAGE_PATH", "./data"),
        auto_digest_enabled=os.getenv("AUTO_DIGEST_ENABLED", "false").lower() in ("true", "1", "yes"),
        auto_digest_mode=os.getenv("AUTO_DIGEST_MODE", "scheduled"),
        auto_digest_hour=int(os.getenv("AUTO_DIGEST_HOUR", "23")),
        auto_digest_interval_hours=int(os.getenv("AUTO_DIGEST_INTERVAL_HOURS", "6")),
        max_context_tokens=int(os.getenv("MAX_CONTEXT_TOKENS", "32768")),
    )


class AppState:
    def __init__(self):
        self._settings: AppSettings = _get_default_settings()
        self._index_stats: IndexStats = IndexStats(
            total_files=0, total_dirs=0, indexed_files=0, indexed_dirs=0, storage_used=0
        )
        self._index_status: IndexStatus = IndexStatus(is_indexing=False, progress=0.0)
        self._rag_service: Optional[RAGService] = None
        self._chat_history: Optional[ChatHistoryService] = None
        self._plan_store: Optional[PlanStore] = None
        self._share_store = None
        self._version_store = None
        self._user_store = None
        self._upload_log = None
        self._favorite_store = None
        self._service_lock = __import__("threading").Lock()
        self._load()

    def _load(self):
        settings_data = _load_json(SETTINGS_FILE, None)
        if settings_data:
            env_defaults = _get_default_settings().model_dump()
            env_defaults.update(settings_data)
            # The thinking budget used to be one global setting, then a
            # global default that providers could override. It now lives
            # on the provider itself, so the old keys have nothing left
            # to carry -- the provider rows were given the same value
            # when the column was added.
            env_defaults.pop("agent_reasoning_budget", None)
            env_defaults.pop("default_reasoning_budget", None)
            self._settings = AppSettings(**env_defaults)
        else:
            self._settings = _get_default_settings()

        # Seed the providers table on first load. Older deployments had
        # llm_base_url / llm_model / llm_api_key (and the llm_fast_*
        # variants) inside AppSettings. We carry those values into the
        # providers table so the agent router finds something to talk
        # to, and we no longer need the settings keys afterwards. The
        # seed is gated on "providers table is empty" so reruns are
        # idempotent and operator-added providers are never overwritten.
        with self.get_chat_history()._lock:  # type: ignore[attr-defined]
            conn = self.get_chat_history()._get_conn()  # type: ignore[attr-defined]
            count = conn.execute("SELECT COUNT(*) FROM llm_providers").fetchone()[0]
            if count == 0:
                self._seed_providers_from_settings(conn, settings_data or {})
            emb_count = conn.execute(
                "SELECT COUNT(*) FROM embedding_providers"
            ).fetchone()[0]
            if emb_count == 0:
                self._seed_embedding_providers_from_settings(
                    conn, settings_data or {}
                )
            conn.close()

        stats_data = _load_json(INDEX_STATS_FILE, None)
        if stats_data:
            self._index_stats = IndexStats(**stats_data)

    def _seed_providers_from_settings(self, conn, settings_data: dict) -> None:
        """One-shot backfill for deployments upgrading from the
        pre-providers version of SimpleFileManager.

        In that version, the LLM endpoint lived in AppSettings
        (llm_base_url / llm_model / llm_api_key) and an optional
        fast model in (llm_fast_base_url / llm_fast_model /
        llm_fast_api_key). AppSettings no longer carries these
        fields, so the persisted settings.json will be empty for
        them; the values that actually drive a deployment now
        live in .env, so the seeder reads them straight from the
        environment. Old settings.json values are still consulted
        first so an in-flight upgrade that has settings.json but
        no env vars does not lose its endpoint.

        Runs inside the caller's lock so the seed and any
        subsequent reads see a consistent table. Idempotent —
        the caller skips when the table is non-empty.
        """
        import time as _t
        now = int(_t.time() * 1000)
        # settings.json wins if it still carries the legacy fields;
        # otherwise fall back to the env vars that LLM_Services
        # / serve.sh already use. The env path is the common case
        # after the refactor; the settings path only matters for
        # a deployment mid-upgrade whose settings.json was written
        # before the field removal.
        main_url = (
            settings_data.get("llm_base_url", "")
            or os.getenv("LLM_BASE_URL", "")
        )
        main_model = (
            settings_data.get("llm_model", "")
            or os.getenv("LLM_MODEL", "")
        )
        main_key = (
            settings_data.get("llm_api_key", "")
            or os.getenv("LLM_API_KEY", "")
        )
        if main_url and main_model:
            main_id = uuid.uuid4().hex
            conn.execute(
                "INSERT INTO llm_providers (id, name, base_url, model, api_key,"
                " is_main, created_at, updated_at) VALUES (?, ?, ?, ?, ?, 1, ?, ?)",
                (main_id, "Main", main_url, main_model, main_key, now, now)
            )
        # Fast model is optional; skip if not configured.
        fast_url = (
            settings_data.get("llm_fast_base_url", "")
            or os.getenv("LLM_FAST_BASE_URL", "")
        )
        fast_model = (
            settings_data.get("llm_fast_model", "")
            or os.getenv("LLM_FAST_MODEL", "")
        )
        fast_key = (
            settings_data.get("llm_fast_api_key", "")
            or os.getenv("LLM_FAST_API_KEY", "")
        )
        if fast_url and fast_model:
            fast_id = uuid.uuid4().hex
            conn.execute(
                "INSERT INTO llm_providers (id, name, base_url, model, api_key,"
                " is_main, created_at, updated_at) VALUES (?, ?, ?, ?, ?, 0, ?, ?)",
                (fast_id, "Fast", fast_url, fast_model, fast_key, now, now)
            )
        conn.commit()

    def _seed_embedding_providers_from_settings(
        self, conn, settings_data: dict,
    ) -> None:
        """One-shot backfill for the embedding providers table.

        Mirrors `_seed_providers_from_settings` for the embedding
        side: in the pre-providers version of SimpleFileManager
        AppSettings carried `embedding_base_url / embedding_model /
        embedding_dim / embedding_api_key` for the indexer, plus
        optional `search_base_url / search_model / search_api_key`
        for the dedicated search path. AppSettings no longer holds
        these fields, so the persisted settings.json will be empty
        for them; the values that actually drive a deployment now
        live in .env. The seeder reads settings.json first (so a
        mid-upgrade deployment doesn't lose its endpoint) and falls
        back to the env vars that LLMServices/serve.sh already
        passes through.

        Runs inside the caller's lock. Idempotent — the caller
        skips when the table is non-empty.
        """
        import time as _t
        now = int(_t.time() * 1000)
        # Indexer: required. settings.json wins; env vars backfill
        # when settings.json was written before the field removal.
        idx_url = (
            settings_data.get("embedding_base_url", "")
            or os.getenv("EMBEDDING_BASE_URL", "")
        )
        idx_model = (
            settings_data.get("embedding_model", "")
            or os.getenv("EMBEDDING_MODEL", "")
        )
        idx_dim = (
            settings_data.get("embedding_dim", "")
            or os.getenv("EMBEDDING_DIM", "AUTO")
        )
        idx_key = (
            settings_data.get("embedding_api_key", "")
            or os.getenv("EMBEDDING_API_KEY", "")
        )
        if idx_url and idx_model:
            idx_id = uuid.uuid4().hex
            conn.execute(
                "INSERT INTO embedding_providers (id, name, base_url, model, dim,"
                " api_key, is_index, is_search, created_at, updated_at)"
                " VALUES (?, ?, ?, ?, ?, ?, 1, 0, ?, ?)",
                (idx_id, "Indexer", idx_url, idx_model, idx_dim,
                 idx_key, now, now)
            )
        # Search: optional. Only seed a second row when the search
        # URL/model differ from the index row — otherwise sharing
        # the index row is the right behaviour and an extra row
        # would just be a duplicate.
        search_url = (
            settings_data.get("search_base_url", "")
            or os.getenv("EMBEDDING_SEARCH_URL", "")
        )
        search_model = (
            settings_data.get("search_model", "")
            or os.getenv("EMBEDDING_SEARCH_MODEL", "")
        )
        search_key = (
            settings_data.get("search_api_key", "")
            or os.getenv("EMBEDDING_SEARCH_API_KEY", "")
        )
        if search_url and search_model and (
            search_url != idx_url or search_model != idx_model
        ):
            search_id = uuid.uuid4().hex
            conn.execute(
                "INSERT INTO embedding_providers (id, name, base_url, model, dim,"
                " api_key, is_index, is_search, created_at, updated_at)"
                " VALUES (?, ?, ?, ?, ?, ?, 0, 1, ?, ?)",
                (search_id, "Search", search_url, search_model, idx_dim,
                 search_key, now, now)
            )
            # The indexer is no longer the search's only backend; flip
            # the search role onto the dedicated row.
            if idx_url and idx_model:
                conn.execute(
                    "UPDATE embedding_providers SET is_search = 1"
                    " WHERE id = ?", (search_id,)
                )
        conn.commit()

    def _persist_settings(self):
        _save_json(SETTINGS_FILE, self._settings.model_dump())

    def _persist_index_stats(self):
        _save_json(INDEX_STATS_FILE, self._index_stats.model_dump())

    def get_settings(self) -> AppSettings:
        return self._settings

    def update_settings(self, updates: dict) -> AppSettings:
        # Snapshot thinking_enabled BEFORE applying updates, so we can
        # tell whether the toggle actually flipped. LFM2.5-A2B does not
        # support request-level thinking-off, so a value change forces
        # a llama-server restart — surfacing the diff here lets the
        # route layer return a "restart required" hint to the UI.
        prev_thinking = self._settings.thinking_enabled
        for k, v in updates.items():
            if hasattr(self._settings, k):
                setattr(self._settings, k, v)
        self._persist_settings()
        self._rag_service = None
        # Drop the cached search embedder too; the next search picks up
        # whatever EMBEDDING_SEARCH_URL now points at.
        self._search_embedding_service = None

        # If the thinking toggle flipped, write the new value to .env
        # and bounce llama-server. We do this even when flipping back to
        # True, because the running server might have been started with
        # the old flag and we want both transitions to go through the
        # same path.
        if "thinking_enabled" in updates and updates["thinking_enabled"] != prev_thinking:
            self._apply_thinking_setting(self._settings.thinking_enabled)

        return self._settings

    def _apply_thinking_setting(self, enabled: bool) -> None:
        """Persist `enabled` to the project .env and bounce llama-server.

        The .env at the repo root is what `LLMServices/serve.sh` reads via
        its `env_value` helper; we update only the THINKING_ENABLED key so
        the rest of the file (DB URLs, INDEX_*, MAX_*, etc.) is left
        untouched. After the write we run `serve.sh restart`, which is
        blocking on the order of 10-20s while the 14 GB GGUF reloads. We
        catch and log any error so a restart failure still leaves the
        setting persisted — the next manual restart will pick it up.
        """
        env_path = BASE_DIR.parent / ".env"
        try:
            _update_env_value(env_path, "THINKING_ENABLED", "true" if enabled else "false")
        except Exception as e:
            print(f"[settings] failed to update .env: {e}")
        serve_sh = BASE_DIR.parent / "LLMServices" / "serve.sh"
        if not serve_sh.is_file():
            print(f"[settings] serve.sh not found at {serve_sh}; skipping restart")
            return
        try:
            import subprocess
            subprocess.run(
                ["bash", str(serve_sh), "restart"],
                cwd=str(serve_sh.parent),
                check=False,
                capture_output=True,
                text=True,
                timeout=120,
            )
        except Exception as e:
            print(f"[settings] serve.sh restart failed: {e}")

    def get_index_stats(self) -> IndexStats:
        self._index_stats.storage_used = _calc_storage_used()
        # Skip counters are per-scan and not persisted; read them live from the
        # indexer so they reflect the most recent pass.
        try:
            from .indexer import indexer as _indexer
            skipped = _indexer.skipped_files()
            self._index_stats.unsupported_count = len(skipped["unsupported"])
            self._index_stats.failed_count = len(skipped["failed"])
        except Exception:
            pass
        return self._index_stats

    def update_index_stats(self, updates: dict) -> IndexStats:
        for k, v in updates.items():
            if hasattr(self._index_stats, k):
                setattr(self._index_stats, k, v)
        self._persist_index_stats()
        return self._index_stats

    def get_index_status(self) -> IndexStatus:
        return self._index_status

    def set_indexing(self, is_indexing: bool, progress: float = 0.0, current_path: Optional[str] = None):
        self._index_status = IndexStatus(
            is_indexing=is_indexing,
            progress=progress,
            current_path=current_path
        )

    def get_rag_service(self) -> RAGService:
        if self._rag_service is None:
            with self._service_lock:
                if self._rag_service is not None:
                    return self._rag_service
                # Indexer is the is_index=1 row. The RAG service is what
                # the indexer runs on, so it shares that endpoint. When no
                # provider is configured the user gets a friendly error
                # from the RAG query endpoint instead of a 500.
                idx = self.get_chat_history().get_index_embedding_provider()
                if idx is None:
                    raise RuntimeError(
                        "没有可用的 embedding provider；请到 /settings 添加"
                    )
                embedding_svc = EmbeddingService(
                    api_key=idx.api_key, base_url=idx.base_url, model=idx.model,
                )
                db_path = str(DATA_DIR / "vector_db")
                self._rag_service = RAGService(
                    embedding_service=embedding_svc,
                    db_path=db_path,
                )
        return self._rag_service

    def get_search_embedding_service(self) -> EmbeddingService:
        """Embedding client for the user-facing search/chat path.

        Reads the is_search=1 row (falling back to the is_index row
        when search is unset, so cloud-only deployments share one
        endpoint). Created lazily and cached on the AppState so
        per-query HTTP connections are reused; the cache is
        invalidated by `invalidate_embedding_caches()` whenever the
        underlying provider row is mutated.
        """
        if getattr(self, "_search_embedding_service", None) is None:
            chat_svc = self.get_chat_history()
            prov = chat_svc.get_search_embedding_provider()
            if prov is None:
                raise RuntimeError(
                    "没有可用的 embedding provider；请到 /settings 添加"
                )
            self._search_embedding_service = EmbeddingService(
                api_key=prov.api_key,
                base_url=prov.base_url,
                model=prov.model,
            )
        return self._search_embedding_service

    def invalidate_embedding_caches(self) -> None:
        """Drop the cached EmbeddingService instances.

        Called after any update_embedding_provider / set-role / delete
        so a fresh request picks up the new endpoint. The RAG service
        (which embeds the indexer-side query vector) and the search
        service share the cache surface; both are dropped together.
        """
        self._search_embedding_service = None
        # Don't drop _rag_service here — the RAG service also owns the
        # LanceDB handle, which is expensive to re-open. The indexer's
        # base_url/model only matters at embed() time, so drop the
        # inner service instead.
        if self._rag_service is not None:
            idx = self.get_chat_history().get_index_embedding_provider()
            if idx is not None:
                self._rag_service.embedding_service = EmbeddingService(
                    api_key=idx.api_key, base_url=idx.base_url, model=idx.model,
                )

    def get_chat_history(self) -> ChatHistoryService:
        if self._chat_history is None:
            db_path = str(DATA_DIR / "chat_history.db")
            self._chat_history = ChatHistoryService(db_path)
        return self._chat_history

    def get_plan_store(self) -> PlanStore:
        if self._plan_store is None:
            db_path = str(DATA_DIR / "plans.db")
            self._plan_store = PlanStore(db_path)
        return self._plan_store

    def get_version_store(self):
        """The history of what files used to contain."""
        if self._version_store is None:
            from .versions import VersionStore

            self._version_store = VersionStore(DATA_DIR / VERSIONS_DIR_NAME)
        return self._version_store

    def get_favorite_store(self):
        """The files somebody wants to keep an eye on."""
        if self._favorite_store is None:
            from .favorites import FAVORITES_DB_NAME, FavoriteStore

            self._favorite_store = FavoriteStore(str(DATA_DIR / FAVORITES_DB_NAME))
        return self._favorite_store

    def get_user_store(self):
        """The users and their grants."""
        if self._user_store is None:
            from .users import UserStore

            self._user_store = UserStore(str(DATA_DIR / USERS_DB_NAME))
        return self._user_store

    def get_upload_log(self):
        """The append-only record of uploads.

        It is not part of the index and not a property of a file: it
        is an event log, and a file that arrived some other way simply
        has no event.
        """
        if self._upload_log is None:
            from .upload_log import UploadLog

            self._upload_log = UploadLog(str(DATA_DIR / "upload_log.db"))
        return self._upload_log

    def get_share_store(self):
        """The shares table.

        Shares live in their own database rather than alongside the
        plans, because their lifetime is different: a plan is part of
        a conversation, a share outlives it and may still be open
        months later.
        """
        if self._share_store is None:
            from .shares import ShareStore

            self._share_store = ShareStore(str(DATA_DIR / "shares.db"))
        return self._share_store


state = AppState()
