import json
from pathlib import Path
from typing import Iterator, Optional

from fastapi import APIRouter, Depends, HTTPException
from fastapi.responses import StreamingResponse
from pydantic import BaseModel

from ..deps import state, can_access, get_storage_root
from .auth import require_admin
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
    # How many documents are in the index. This is the number a user
    # means by "已索引" -- the one that used to be the chunk count,
    # which read as several thousand indexed files when there were
    # four.
    indexed_count: int
    # How many pieces those documents were broken into.
    vector_count: int
    total_chunks: int = 0
    semantic_percent: int = 0
    # How many documents are in the keyword index but not yet in the
    # vector one.
    keyword_only: int = 0


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
def index_file(req: IndexRequest, _admin=Depends(require_admin)) -> dict:
    try:
        rag_svc = state.get_rag_service()
        rag_svc.index_file(req.file_path, req.content)
        search_engine.upsert_file(req.file_path, req.content)
        return {"success": True, "message": "File indexed"}
    except Exception as e:
        raise HTTPException(status_code=500, detail=str(e))


@rag.post("/index_batch")
def index_files(req: IndexBatchRequest, _admin=Depends(require_admin)) -> dict:
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
    # Embedding a query hits the same BGE server the indexer is using, so
    # mark the user active and let indexing stand down for ~45s. Without this
    # the search has to wait for the indexer's in-flight 12-chunk batch to
    # finish (15-20s on a busy server). Same idea as /api/search/hybrid.
    # The query embedding itself goes through the dedicated *search* embedder
    # (EMBEDDING_SEARCH_URL) so it has its own low-latency path even while
    # the indexer is hammering the indexer embedder.
    from ..interaction import interaction
    interaction.mark_active()
    try:
        search_embedder = state.get_search_embedding_service()
        rag_svc = state.get_rag_service()
        query_vector = search_embedder.embed([req.query])[0]
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
    # RAG query embeds a question and then runs chat completion -- both compete
    # for the embedding server and the LLM. Mark active so the indexer
    # stands down for the duration of the answer.
    from ..interaction import interaction
    interaction.mark_active()
    try:
        rag_svc = state.get_rag_service()
        # Use the search embedder without mutating the shared singleton.
        search_svc = state.get_search_embedding_service()
        result = rag_svc.query(req.question, top_k=req.top_k, embedder=search_svc)
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
    from ..interaction import interaction
    # Mark before yielding the SSE stream starts so the indexer yields before
    # the next batch boundary even on a long-running streamed answer.
    interaction.mark_active()
    def sse(event: str, data: dict) -> str:
        return f"event: {event}\ndata: {json.dumps(data, ensure_ascii=False)}\n\n"

    def generate() -> Iterator[str]:
        try:
            rag_svc = state.get_rag_service()
            search_svc = state.get_search_embedding_service()
            yield sse("meta", {"status": "searching"})
            results, context = rag_svc._retrieve_for_answer(req.question, top_k=req.top_k, embedder=search_svc)

            if not results:
                yield sse("done", {"answer": "没有找到相关文件来回答这个问题。", "sources": []})
                return

            yield sse("sources", {
                "sources": [{"file_path": r.get("file_path", ""), "score": r.get("score", 0)} for r in results]
            })

            from ..llm_client import chat_completion_stream
            rag_provider = state.get_chat_history().get_main_provider()
            answer = []
            for piece in chat_completion_stream(
                rag_svc._answer_messages(req.question, context),
                temperature=0.7,
                provider=rag_provider,
                reasoning_budget=rag_provider.reasoning_budget,
            ):
                # The generator yields (kind, payload) pairs. Forwarding only
                # "token" silently discarded the model's reasoning channel; it
                # now gets its own event so the UI can show how the summary was
                # built from the retrieved passages.
                kind, payload = piece
                if kind == "reasoning":
                    yield sse("reasoning", {"text": payload})
                elif kind == "token":
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
        total_chunks = sum(t for _d, t, _sig in progress.values() if t > 0)
        # Clamped: coverage above 100% means the manifest has drifted from the
        # vector store, and a number like 101% reads as a bug rather than a hint.
        percent = min(100, round(embedded * 100 / total_chunks)) if total_chunks else 0
        # The number of documents, rather than the number of pieces they
        # were cut into. The two are wildly different: four novels are
        # four documents and nearly ten thousand chunks.
        indexed_files = 0
        keyword_files = 0
        try:
            from ..search_engine import search_engine
            keyword_files = len(search_engine.indexed_file_names())
        except Exception:
            pass
        try:
            indexed_files = len(rag_svc.vector_store.list_files())
        except Exception:
            indexed_files = 0
        return IndexStatusResponse(
            indexed_count=indexed_files,
            vector_count=embedded,
            total_chunks=total_chunks,
            semantic_percent=percent,
            keyword_only=max(0, keyword_files - indexed_files),
        )
    except Exception as e:
        raise HTTPException(status_code=500, detail=str(e))


def _indexed_file_is_readable(file_path: str) -> bool:
    """Whether an indexed document is one the caller may open."""
    if not file_path:
        return False
    target = Path(file_path)
    if not target.is_absolute():
        target = get_storage_root() / file_path
    return can_access(target, "read")


@rag.get("/files", response_model=IndexedFilesResponse)
def get_indexed_files():
    try:
        rag_svc = state.get_rag_service()
        files = rag_svc.vector_store.list_files()

        # The index covers the whole machine. A person sees only the
        # documents they are allowed to open.
        files = [f for f in files if _indexed_file_is_readable(f.get("file_path", ""))]

        # Join in the indexer's progress so the UI can show how much of each
        # document has actually been embedded, not just how many chunks exist.
        from ..indexer import CHUNK_SIG, indexer as _indexer
        progress = _indexer._vector_progress()
        for f in files:
            done, total, _sig = progress.get(
                f["file_path"],
                (f.get("chunk_count", 0), f.get("chunk_count", 0), CHUNK_SIG),
            )
            f["total_chunks"] = total
            f["percent"] = min(100, round(done * 100 / total)) if total else 100

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
