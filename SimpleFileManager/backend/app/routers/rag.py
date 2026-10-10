from typing import Optional

from fastapi import APIRouter, HTTPException
from pydantic import BaseModel

from ..deps import state
from ..search_engine import search_engine

rag = APIRouter()


class IndexRequest(BaseModel):
    file_path: str
    content: str


class IndexBatchRequest(BaseModel):
    files: list[dict]


class QueryRequest(BaseModel):
    question: str
    top_k: int = 3


class SearchRequest(BaseModel):
    query: str
    top_k: int = 5


class SearchResult(BaseModel):
    items: list[dict]
    total: int
    query: str


class QueryResponse(BaseModel):
    answer: str
    sources: list[dict]


class IndexStatusResponse(BaseModel):
    indexed_count: int
    vector_count: int
    total_chunks: int = 0
    semantic_percent: int = 0


class IndexedFile(BaseModel):
    id: str
    file_path: str
    content_preview: str
    # A chunked document contributes many rows upstream; they are collapsed
    # into one entry so the list and the delete action stay per-file.
    chunk_count: int = 1
    # Chunks embedded so far / total chunks the file splits into.
    total_chunks: int = 1
    percent: int = 100


class IndexedFilesResponse(BaseModel):
    files: list[IndexedFile]
    total: int


@rag.post("/index")
def index_file(req: IndexRequest) -> dict:
    try:
        rag_svc = state.get_rag_service()
        rag_svc.index_file(req.file_path, req.content)
        search_engine.upsert_file(req.file_path, req.content)
        return {"success": True, "message": "File indexed"}
    except Exception as e:
        raise HTTPException(status_code=500, detail=str(e))


@rag.post("/index_batch")
def index_files(req: IndexBatchRequest) -> dict:
    try:
        rag_svc = state.get_rag_service()
        rag_svc.index_files(req.files)
        for f in req.files:
            search_engine.upsert_file(f.get("file_path", ""), f.get("content", ""))
        return {"success": True, "count": len(req.files)}
    except Exception as e:
        raise HTTPException(status_code=500, detail=str(e))


@rag.post("/search")
def semantic_search(req: SearchRequest) -> SearchResult:
    try:
        rag_svc = state.get_rag_service()
        query_vector = rag_svc.embedding_service.embed([req.query])[0]
        results = rag_svc.vector_store.search(query_vector, top_k=req.top_k)
        return SearchResult(
            items=results,
            total=len(results),
            query=req.query,
        )
    except Exception as e:
        raise HTTPException(status_code=500, detail=str(e))


@rag.post("/query")
def query(req: QueryRequest) -> QueryResponse:
    try:
        rag_svc = state.get_rag_service()
        result = rag_svc.query(req.question, top_k=req.top_k)
        return QueryResponse(**result)
    except Exception as e:
        raise HTTPException(status_code=500, detail=str(e))


@rag.get("/status")
def get_status() -> IndexStatusResponse:
    try:
        rag_svc = state.get_rag_service()
        embedded = rag_svc.vector_store.count()
        # Semantic coverage across everything the manifest knows about, which is
        # what makes slow embedding visible instead of looking finished.
        from ..indexer import indexer as _indexer
        progress = _indexer._vector_progress()
        total_chunks = sum(t for _d, t in progress.values() if t > 0)
        percent = round(embedded * 100 / total_chunks) if total_chunks else 0
        return IndexStatusResponse(
            indexed_count=embedded,
            vector_count=embedded,
            total_chunks=total_chunks,
            semantic_percent=percent,
        )
    except Exception as e:
        raise HTTPException(status_code=500, detail=str(e))


@rag.get("/files", response_model=IndexedFilesResponse)
def get_indexed_files():
    try:
        rag_svc = state.get_rag_service()
        files = rag_svc.vector_store.list_files()

        # Join in the indexer's progress so the UI can show how much of each
        # document has actually been embedded, not just how many chunks exist.
        from ..indexer import indexer as _indexer
        progress = _indexer._vector_progress()
        for f in files:
            done, total = progress.get(f["file_path"], (f.get("chunk_count", 0), f.get("chunk_count", 0)))
            f["total_chunks"] = total
            f["percent"] = round(done * 100 / total) if total else 100

        return IndexedFilesResponse(
            files=[IndexedFile(**f) for f in files],
            total=len(files)
        )
    except Exception as e:
        raise HTTPException(status_code=500, detail=str(e))


@rag.delete("/files/{file_path:path}")
def delete_indexed_file(file_path: str) -> dict:
    try:
        rag_svc = state.get_rag_service()
        rag_svc.vector_store.delete_by_file(file_path)
        search_engine.remove_file(file_path)
        return {"success": True}
    except Exception as e:
        raise HTTPException(status_code=500, detail=str(e))


@rag.delete("/clear")
def clear_index() -> dict:
    try:
        rag_svc = state.get_rag_service()
        rag_svc.clear_index()
        search_engine.clear()
        return {"success": True, "message": "Index cleared"}
    except Exception as e:
        raise HTTPException(status_code=500, detail=str(e))
