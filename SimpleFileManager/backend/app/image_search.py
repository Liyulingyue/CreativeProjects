"""Finding pictures by what they look like.

Two ways in, one table:

  * a sentence -- "a red apple on a table" -- is encoded with jina and
    compared against the picture vectors;
  * a picture -- "find more like this one" -- is encoded the same way and
    compared against the same vectors.

Both go through jina-embeddings-v5-omni, which puts pictures and text in
one vector space. The pictures have to have been encoded first, which is
what the picture index is for; that job is deliberate (a button or a
schedule), because a picture costs about ten seconds of the big cores.

The big cores have one owner at a time: the model, this search, or the
picture index. A search that cannot get the cores within a few seconds
gives up and says so -- someone waiting for a picture does not want to be
queued behind a build.
"""

import logging
import time
from pathlib import Path
from typing import Optional

from .cpu_pool import get_big_pool
from .interaction import interaction
from .image_embed import JinaError, get_image_embedder, is_image_file
from .image_store import ImageVectorStore, get_image_store

log = logging.getLogger(__name__)

# How long a search waits for the big cores.
#
# A *text* query is encoded in a fraction of a second and does not need
# the whole box: it takes a reader's lease and runs next to a
# conversation that is being answered. It still has to wait for the
# picture index, which holds the box for a whole picture at a time.
#
# A *picture* query has to encode the picture itself -- seconds of every
# core -- so it takes a writer's lease, and how long that takes depends
# on the model and the projector (three seconds with the low-resolution
# one, ten with the shipped one). The indexer publishes what it last
# measured, and the search waits for that much.
#
# Neither of them queues for long: a person waiting for a picture gets
# an answer or a clear refusal, not a spinner.
SEARCH_LOCK_TIMEOUT = 2.0
SEARCH_LOCK_MARGIN = 2.0
# However slow a picture is, a search does not wait longer than this.
SEARCH_LOCK_MAX = 20.0


def _check_vision(embedder, what: str) -> None:
    """Say something useful when the picture model has no projector.

    A jina service started without the vision projector answers normally
    and only fails when a picture is sent to it, with a message about
    bitmaps and markers that means nothing to the person reading it.
    """
    try:
        if embedder.supports_images():
            return
    except Exception:  # noqa: BLE001
        return
    raise SearchUnavailable(
        f"{what}不可用：图片模型没有视觉投影器"
        f"（serve.sh 的 JINA_MMPROJ 指向的文件不存在，或不是 -64tok 那份）")


def _search_timeout(encoding_a_picture: bool = False) -> float:
    """How long a search should wait for the big cores.

    A text query is encoded in a fraction of a second, so it waits a
    couple of seconds and then says so. A picture query encodes the
    picture itself -- seconds of every core -- so it waits for as long
    as that takes.
    """
    if not encoding_a_picture:
        return SEARCH_LOCK_TIMEOUT
    try:
        from .image_indexer import image_indexer
        measured = float(image_indexer.status().get("last_encode_seconds") or 0)
    except Exception:  # noqa: BLE001
        measured = 0.0
    if measured <= 0:
        return SEARCH_LOCK_TIMEOUT
    # Bounded on both sides: long enough for a slow picture, short
    # enough that a person waiting for a search is not left staring at a
    # spinner.
    return min(SEARCH_LOCK_MAX,
               max(SEARCH_LOCK_TIMEOUT, measured * 1.5 + SEARCH_LOCK_MARGIN))

# The biggest picture that may be used as a reference. The picture is
# read, base64'd and serialized into JSON, so it is held four times over
# while the request is built.
MAX_SOURCE_BYTES = 20 * 1024 * 1024

# The prefix jina's own examples use for a retrieval query.
QUERY_PREFIX = "Query: "


class SearchUnavailable(Exception):
    """The big cores were busy and the search gave up."""


def _storage_root() -> Path:
    from .deps import get_storage_root
    return get_storage_root()


def _readable(hits: list[dict]) -> tuple[list[dict], list[str]]:
    """Drop the hits the caller may not open, and the ones that are gone.

    The picture index is built from the whole storage root, so it holds
    everything -- including what one person may not look at. The text
    search filters its results the same way; without this, asking for "a
    photo of a red apple" would hand back the name and size of a private
    folder's picture.

    The second reason is a picture that was deleted since the index was
    built. The index is built once a day, so a picture deleted in the
    morning would still be handed out in the evening -- with its name,
    its size and a thumbnail that does not draw. The row is dropped
    here, and the caller is told which ones so that the table can be
    cleaned of them.
    """
    from .deps import can_access

    root = _storage_root()
    out = []
    gone: list[str] = []
    for hit in hits:
        path = hit.get("file_path") or hit.get("path") or ""
        if not path:
            continue
        try:
            if not can_access(root / path, "read"):
                continue
            if not (root / path).exists():
                gone.append(path)
                continue
        except OSError:
            continue
        out.append(hit)
    return out, gone


def _hits(store, vector: list[float], wanted: int) -> list[dict]:
    """The best pictures for a vector, without the ones that are gone.

    The table is asked for more than the caller wants, because some of
    the nearest pictures may be in a folder this user cannot open, or
    may have been deleted since the index was built. When those are
    filtered away the page would show fewer than it should, so the
    table is asked again with a wider net -- and the rows of the
    pictures that are gone are dropped, so the table does not fill up
    with names of files that are not there.
    """
    asked = min(max(wanted * 4, 12), 200)
    keep: list[dict] = []
    seen_gone: set[str] = set()
    while asked <= 800:
        hits = store.search(vector, top_k=asked)
        readable, gone = _readable(hits)
        for path in gone:
            if path not in seen_gone:
                seen_gone.add(path)
                try:
                    store.delete_by_file(path)
                except Exception:  # noqa: BLE001
                    # The row stays; the next search will notice it again.
                    pass
        keep = readable
        if len(readable) >= wanted or len(hits) < asked:
            break
        asked *= 4
    return keep[:wanted]


def _image_store() -> ImageVectorStore:
    from .deps import DATA_DIR
    return get_image_store(str(DATA_DIR / "vectors"))


def _describe(path: str) -> dict:
    """What the caller gets to know about a hit."""
    p = Path(path)
    size = 0
    modified = 0.0
    try:
        # The path is relative to the storage root, not to the working
        # directory of the server.
        stat = (_storage_root() / path).stat()
        size = stat.st_size
        modified = stat.st_mtime
    except OSError:
        pass
    return {
        "path": path,
        "name": p.name,
        "size": size,
        "modified_at": modified,
    }


def search_by_text(query: str, limit: int = 12) -> list[dict]:
    """The pictures that best match a sentence."""
    query = (query or "").strip()
    if not query:
        raise ValueError("检索词不能为空")

    store = _image_store()
    if store.count() == 0:
        return []

    # A search is an interaction: the index build stands down for it, the
    # same way it stands down for a conversation.
    interaction.mark_active()

    embedder = get_image_embedder()
    _check_vision(embedder, "以文搜图")
    # A text query is a fraction of a second of work: it takes a
    # reader's lease, so it does not have to wait for a conversation to
    # finish. It does have to wait for the picture index, which is
    # holding the box for a whole picture at a time.
    with get_big_pool().try_hold(timeout=_search_timeout(), shared=True) as got:
        if not got:
            raise SearchUnavailable(
                "服务器正在忙（模型生成或图片索引构建中），以文搜图暂时不可用，请稍后重试"
            )
        try:
            vector = embedder.embed_text(query, prefix=QUERY_PREFIX)
        except JinaError as e:
            raise SearchUnavailable(f"图片模型不可用：{e}") from e

    # Ask for more than the caller wants: some of the nearest pictures
    # may be in a folder this user cannot open, and filtering them away
    # afterwards would otherwise leave the page empty while plenty of
    # reachable pictures match.
    wanted = max(1, min(limit, 50))
    hits = _hits(store, vector, wanted)
    return [{"score": round(h.get("score", 0.0), 4), **_describe(h.get("file_path", ""))}
            for h in hits]


def search_by_image(image_path: str, limit: int = 12) -> list[dict]:
    """The pictures that look most like a given picture."""
    from .deps import PathNotAllowed, PathOutsideRoot, resolve_under_root

    # The path comes from the model or from a request body, so it is
    # resolved the same way every other route resolves one: it has to be
    # inside the storage root, and the caller has to be allowed to open it.
    try:
        source = resolve_under_root(image_path)
    except (PathOutsideRoot, PathNotAllowed) as e:
        raise ValueError(f"没有权限访问这张图片：{image_path}") from e
    if not source.exists() or not is_image_file(source):
        raise ValueError(f"不是一张能检索的图片：{image_path}")
    # The picture is read, base64'd and serialized into JSON -- four times
    # its size in memory at once. A two-gigabyte picture would take the
    # whole server down with it.
    try:
        size = source.stat().st_size
    except OSError:
        size = 0
    if size > MAX_SOURCE_BYTES:
        raise ValueError(f"图片太大（{size // 1024 // 1024}MB），无法用来检索")

    store = _image_store()
    if store.count() == 0:
        return []

    # A search is an interaction: the index build stands down for it, the
    # same way it stands down for a conversation.
    interaction.mark_active()

    embedder = get_image_embedder()
    _check_vision(embedder, "以图搜图")
    # The picture has to be encoded: seconds of every core. That is a
    # writer's lease, so a conversation has to wait for it.
    with get_big_pool().try_hold(timeout=_search_timeout(True)) as got:
        if not got:
            raise SearchUnavailable(
                "服务器正在忙（模型生成或图片索引构建中），以图搜图暂时不可用，请稍后重试"
            )
        try:
            vector = embedder.embed_image(source)
        except JinaError as e:
            raise SearchUnavailable(f"图片模型不可用：{e}") from e

    wanted = max(1, min(limit, 50))
    hits = _hits(store, vector, wanted)
    results = []
    for h in hits:
        item = {"score": round(h.get("score", 0.0), 4), **_describe(h.get("file_path", ""))}
        if item["path"] == image_path:
            item["is_source"] = True
        results.append(item)
    return results[:wanted]


