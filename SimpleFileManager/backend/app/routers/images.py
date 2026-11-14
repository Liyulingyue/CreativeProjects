"""The picture index and the picture searches that use it.

The index is a deliberate job: a picture costs about ten seconds of the
server's main cores to encode, so the user starts it (or a schedule does)
rather than having it run in the background all the time. While it runs, the
pictures it is building cannot be searched for -- the search needs the same
cores -- and the status endpoint says so.

The searches are here as well as in the agent's tool set, so the web page
can show the pictures it finds without going through a conversation.
"""

from typing import Optional

from fastapi import APIRouter, Depends, HTTPException
from pydantic import BaseModel

from ..image_indexer import image_indexer
from ..image_search import SearchUnavailable, search_by_image, search_by_text
from .auth import require_admin
from .thumbnails import issue_directory_token

images = APIRouter(prefix="/api/images", tags=["images"])


def _with_thumbnails(results: list[dict]) -> list[dict]:
    """Give every hit the key its little picture needs.

    A picture element cannot send an Authorization header, so the browser
    asks for the thumbnail with a key for the directory the picture is in.
    """
    from ..deps import resolve_under_root

    out = []
    for item in results:
        path = item.get("path", "")
        token = ""
        try:
            # The key is signed with the resolved directory, so the string
            # has to be the one the thumbnail endpoint will compute.
            resolved = resolve_under_root(path)
            token = issue_directory_token(str(resolved.parent))["token"]
        except Exception:  # noqa: BLE001 - a missing thumbnail is not a failure
            token = ""
        out.append({**item, "thumbnail_token": token})
    return out


class StartIndex(BaseModel):
    limit: int = 0
    rebuild: bool = False


class TextQuery(BaseModel):
    query: str
    top_k: int = 12


class ImageQuery(BaseModel):
    path: str
    top_k: int = 12


@images.get("/index/status")
def index_status() -> dict:
    """How much of the picture index is built."""
    state = image_indexer.status()
    from ..image_store import get_image_store
    from ..deps import DATA_DIR

    store = get_image_store(str(DATA_DIR / "vectors"))
    return {**state, "indexed": store.count()}


@images.post("/index/start")
def index_start(body: StartIndex, _admin=Depends(require_admin)) -> dict:
    """Start building the picture index."""
    return image_indexer.start(limit=body.limit, rebuild=body.rebuild)


@images.post("/index/pause")
def index_pause(_admin=Depends(require_admin)) -> dict:
    """Stop after the current picture and give the cores back."""
    return image_indexer.pause()


@images.post("/index/resume")
def index_resume(_admin=Depends(require_admin)) -> dict:
    return image_indexer.resume()


@images.post("/index/stop")
def index_stop(_admin=Depends(require_admin)) -> dict:
    return image_indexer.stop()


@images.post("/search")
def search_text(body: TextQuery) -> dict:
    """Pictures that match a description."""
    try:
        results = search_by_text(body.query, limit=body.top_k)
    except SearchUnavailable as e:
        raise HTTPException(status_code=503, detail=str(e))
    except ValueError as e:
        raise HTTPException(status_code=400, detail=str(e))
    return {"results": _with_thumbnails(results)}


@images.post("/search/similar")
def search_similar(body: ImageQuery) -> dict:
    """Pictures that look like a given picture."""
    try:
        results = search_by_image(body.path, limit=body.top_k)
    except SearchUnavailable as e:
        raise HTTPException(status_code=503, detail=str(e))
    except ValueError as e:
        raise HTTPException(status_code=400, detail=str(e))
    return {"results": _with_thumbnails(results)}
