import json
from typing import Iterator, Optional

from fastapi import APIRouter, HTTPException
from fastapi.responses import StreamingResponse
from pydantic import BaseModel

from ..deps import state
from ..llm_client import _friendly_error
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


@rag.post("/query/stream")
def query_stream(req: QueryRequest) -> StreamingResponse:
    """Stream the grounded answer.

    On this CPU-only setup the model generates ~30 tok/s, so a full answer took
    well over a minute behind an unresponsive spinner. Streaming does not make
    it faster but makes the wait legible.
    """
    def sse(event: str, data: dict) -> str:
        return f"event: {event}\ndata: {json.dumps(data, ensure_ascii=False)}\n\n"

    def generate() -> Iterator[str]:
        try:
            rag_svc = state.get_rag_service()
            yield sse("meta", {"status": "searching"})
            results, context = rag_svc._retrieve_for_answer(req.question, top_k=req.top_k)

            if not results:
                yield sse("done", {"answer": "没有找到相关文件来回答这个问题。", "sources": []})
                return

            yield sse("sources", {
                "sources": [{"file_path": r.get("file_path", ""), "score": r.get("score", 0)} for r in results]
            })

            from ..llm_client import chat_completion_stream
            answer = []
            for piece in chat_completion_stream(rag_svc._answer_messages(req.question, context), temperature=0.7):
                # The generator yields (kind, payload) pairs; only the text
                # payload belongs in a token event.
                kind, payload = piece
                if kind != "token":
                    continue
                answer.append(payload)
                yield sse("token", {"text": payload})
            yield sse("done", {"answer": "".join(answer)})
        except GeneratorExit:
            raise
        except Exception as e:
            yield sse("error", {"message": _friendly_error(e)})

    return StreamingResponse(
        generate(),
        media_type="text/event-stream",
        headers={"Cache-Control": "no-cache, no-transform", "X-Accel-Buffering": "no"},
    )


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
