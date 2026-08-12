import json
import os
import time
from pathlib import Path
from typing import Any, Optional

import httpx
from dotenv import load_dotenv
from .models import AppSettings, IndexStats, IndexStatus

load_dotenv()

BASE_DIR = Path(__file__).resolve().parent.parent
STORAGE_PATH = os.getenv("STORAGE_PATH", "./data")
ROOT_DIR = BASE_DIR / STORAGE_PATH
ROOT_DIR.mkdir(exist_ok=True)
DATA_DIR = ROOT_DIR / ".simplefilemanager"
DATA_DIR.mkdir(exist_ok=True)

SETTINGS_FILE = DATA_DIR / "settings.json"
INDEX_STATS_FILE = DATA_DIR / "index_stats.json"

EMBEDDING_DIM_MAP = {
    "text-embedding-3-small": 1536,
    "text-embedding-3-large": 3072,
    "text-embedding-ada-002": 1538,
}


def _detect_embedding_dim(model: str, api_key: str, base_url: str) -> int:
    try:
        import httpx
        headers = {"Authorization": f"Bearer {api_key}"} if api_key else {}
        resp = httpx.post(
            base_url,
            headers=headers,
            json={"input": "test", "model": model},
            timeout=10,
        )
        if resp.status_code == 200:
            data = resp.json()
            embedding = data.get("data", [{}])[0]
            embedding_vector = embedding.get("embedding", [])
            if embedding_vector:
                return len(embedding_vector)
        for known_model, dim in EMBEDDING_DIM_MAP.items():
            if model.startswith(known_model):
                return dim
        return 1536
    except Exception:
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
        headers = {"Authorization": f"Bearer {self.api_key}"} if self.api_key else {}
        resp = httpx.post(
            self.base_url,
            headers=headers,
            json={"input": texts, "model": self.model},
            timeout=30,
        )
        resp.raise_for_status()
        data = resp.json()
        return [item["embedding"] for item in data.get("data", [])]


class LanceDBVectorStore:
    def __init__(self, db_path: str):
        import lancedb
        self.db_path = db_path
        self.db = lancedb.connect(db_path)
        self._table = None
        self._ensure_table()

    def _ensure_table(self):
        import lancedb
        import pyarrow as pa
        if "file_embeddings" not in self.table_names():
            schema = pa.schema([
                ("id", pa.string()),
                ("file_path", pa.string()),
                ("content", pa.string()),
                ("vector", pa.list_(pa.float32())),
            ])
            self.db.create_table("file_embeddings", schema=schema, mode="create")
        self._table = self.db.open_table("file_embeddings")

    def table_names(self) -> list[str]:
        return self.db.table_names()

    def add(self, vector: list[float], metadata: dict):
        import uuid
        self._table.add([{
            "id": uuid.uuid4().hex,
            "file_path": metadata.get("file_path", ""),
            "content": metadata.get("content", ""),
            "vector": vector,
        }])

    def search(self, query_vector: list[float], top_k: int = 5) -> list[dict]:
        if self.count() == 0:
            return []
        results = self._table.search(query_vector, vector_column_name="vector").limit(top_k).to_list()
        return [{"score": 1.0 - r.get("_distance", 0), **r} for r in results]

    def delete_by_file(self, file_path: str):
        self._table.delete(f'file_path = "{file_path}"')

    def clear(self):
        if "file_embeddings" in self.table_names():
            self.db.drop_table("file_embeddings")
        self._ensure_table()

    def count(self) -> int:
        if "file_embeddings" in self.table_names():
            return len(self._table.to_pandas())
        return 0


class RAGService:
    def __init__(self, embedding_service: EmbeddingService, llm_api_key: str, llm_base_url: str, llm_model: str, db_path: str):
        self.embedding_service = embedding_service
        self.llm_api_key = llm_api_key
        self.llm_base_url = llm_base_url
        self.llm_model = llm_model
        self.vector_store = LanceDBVectorStore(db_path)

    def index_file(self, file_path: str, content: str):
        vectors = self.embedding_service.embed([content])
        self.vector_store.add(vectors[0], {"file_path": file_path, "content": content})

    def index_files(self, files: list[dict]):
        contents = [f["content"] for f in files]
        vectors = self.embedding_service.embed(contents)
        for f, vec in zip(files, vectors):
            self.vector_store.add(vec, {"file_path": f["file_path"], "content": f["content"]})

    def query(self, question: str, top_k: int = 3) -> dict:
        query_vector = self.embedding_service.embed([question])[0]
        results = self.vector_store.search(query_vector, top_k=top_k)

        if not results:
            return {"answer": "没有找到相关文件来回答这个问题。", "sources": []}

        context_parts = []
        for i, r in enumerate(results, 1):
            context_parts.append(f"【文档{i}】({r.get('file_path', 'unknown')}):\n{r.get('content', '')[:500]}")
        context = "\n\n".join(context_parts)

        prompt = f"""基于以下文档内容回答问题。如果文档中没有相关信息，请说明无法回答。

问题: {question}

文档内容:
{context}

回答:"""

        headers = {"Authorization": f"Bearer {self.llm_api_key}"} if self.llm_api_key else {}
        resp = httpx.post(
            self.llm_base_url,
            headers=headers,
            json={
                "model": self.llm_model,
                "messages": [
                    {"role": "system", "content": "你是一个基于文档的问答助手。请根据提供的文档内容回答问题。"},
                    {"role": "user", "content": prompt}
                ],
                "temperature": 0.7,
            },
            timeout=60,
        )
        resp.raise_for_status()
        data = resp.json()
        answer = data["choices"][0]["message"]["content"]

        return {
            "answer": answer,
            "sources": [{"file_path": r.get("file_path", ""), "score": r.get("score", 0)} for r in results]
        }

    def clear_index(self):
        self.vector_store.clear()


def _get_default_settings() -> AppSettings:
    embedding_dim_str = os.getenv("EMBEDDING_DIM", "AUTO")
    embedding_api_key = os.getenv("EMBEDDING_API_KEY", "")
    embedding_base_url = os.getenv("EMBEDDING_BASE_URL", "https://api.minimaxi.com/v1")
    embedding_model = os.getenv("EMBEDDING_MODEL", "text-embedding-3-small")

    if embedding_dim_str == "AUTO":
        embedding_dim_str = str(_detect_embedding_dim(embedding_model, embedding_api_key, embedding_base_url))

    return AppSettings(
        llm_api_key=os.getenv("LLM_API_KEY", ""),
        llm_base_url=os.getenv("LLM_BASE_URL", "https://api.minimaxi.com/v1"),
        llm_model=os.getenv("LLM_MODEL", "gpt-4o-mini"),
        embedding_api_key=embedding_api_key,
        embedding_base_url=embedding_base_url,
        embedding_model=embedding_model,
        embedding_dim=embedding_dim_str,
        index_interval=int(os.getenv("INDEX_INTERVAL", "300")),
        storage_path=os.getenv("STORAGE_PATH", "./data"),
    )


class AppState:
    def __init__(self):
        self._settings: AppSettings = _get_default_settings()
        self._index_stats: IndexStats = IndexStats(
            total_files=0, total_dirs=0, indexed_files=0, indexed_dirs=0, storage_used=0
        )
        self._index_status: IndexStatus = IndexStatus(is_indexing=False, progress=0.0)
        self._rag_service: Optional[RAGService] = None
        self._load()

    def _load(self):
        settings_data = _load_json(SETTINGS_FILE, None)
        if settings_data:
            self._settings = AppSettings(**settings_data)

        stats_data = _load_json(INDEX_STATS_FILE, None)
        if stats_data:
            self._index_stats = IndexStats(**stats_data)

    def _persist_settings(self):
        _save_json(SETTINGS_FILE, self._settings.model_dump())

    def _persist_index_stats(self):
        _save_json(INDEX_STATS_FILE, self._index_stats.model_dump())

    def get_settings(self) -> AppSettings:
        return self._settings

    def update_settings(self, updates: dict) -> AppSettings:
        for k, v in updates.items():
            if hasattr(self._settings, k):
                setattr(self._settings, k, v)
        self._persist_settings()
        self._rag_service = None
        return self._settings

    def get_index_stats(self) -> IndexStats:
        self._index_stats.storage_used = _calc_storage_used()
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
            embedding_svc = EmbeddingService(
                api_key=self._settings.embedding_api_key,
                base_url=self._settings.embedding_base_url,
                model=self._settings.embedding_model,
            )
            db_path = str(DATA_DIR / "vector_db")
            self._rag_service = RAGService(
                embedding_service=embedding_svc,
                llm_api_key=self._settings.llm_api_key,
                llm_base_url=self._settings.llm_base_url,
                llm_model=self._settings.llm_model,
                db_path=db_path,
            )
        return self._rag_service


state = AppState()
