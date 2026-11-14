"""Encode text and images with jina-embeddings-v5-omni.

The v5-omni model puts pictures and text in one vector space, so a text
query can find a picture. Text is a normal embedding call; a picture needs
the fork's own request format: the server invents a placeholder token for
one piece of media at startup, and the picture travels beside the prompt as
base64. Guessing the placeholder does not work -- a prompt that carries the
wrong one is rejected with "number of bitmaps (1) does not match number of
markers (0)" -- so it is read back from the server's /props.

The model is slow on this box (a picture costs about three seconds with the
low-resolution projector, nine with the shipped one), which is why the caller
is expected to hold the shared CPU pool while encoding.
"""

import base64
import logging
import os
import threading
from pathlib import Path
from typing import Any, Optional

log = logging.getLogger(__name__)

# The picture formats the vision tower can open. Kept in step with the
# thumbnailer, which renders the same set.
IMAGE_EXTENSIONS = {".jpg", ".jpeg", ".png", ".gif", ".webp", ".bmp", ".tif", ".tiff"}

DEFAULT_BASE_URL = "http://127.0.0.1:9014"
# One picture costs a few seconds on the big cores, so the timeout has to
# be generous; a queue of pictures goes through one call each. It also
# has to be finite: a jina that has stopped answering would otherwise
# hold the cores for three minutes per picture.
DEFAULT_TIMEOUT = 60.0


class JinaError(RuntimeError):
    """The jina service refused or could not be reached."""


class JinaEmbedder:
    """A client for one jina llama-server."""

    def __init__(self, base_url: str = DEFAULT_BASE_URL, timeout: float = DEFAULT_TIMEOUT):
        self.base_url = base_url.rstrip("/")
        self.timeout = timeout
        self._marker: Optional[str] = None
        self._marker_lock = threading.Lock()

    # -- plumbing ----------------------------------------------------------

    def _post(self, path: str, payload: dict) -> Any:
        import httpx2 as httpx

        url = f"{self.base_url}{path}"
        try:
            resp = httpx.post(url, json=payload, timeout=self.timeout)
        except Exception as e:
            raise JinaError(f"无法连接 jina 服务 {self.base_url}: {e}") from e
        if resp.status_code != 200:
            detail = resp.text[:300]
            raise JinaError(f"jina 服务返回 {resp.status_code}: {detail}")
        try:
            return resp.json()
        except Exception as e:
            raise JinaError("jina 服务返回的不是 JSON") from e

    def _get(self, path: str) -> Any:
        import httpx2 as httpx

        try:
            resp = httpx.get(f"{self.base_url}{path}", timeout=min(self.timeout, 10.0))
        except Exception as e:
            raise JinaError(f"无法连接 jina 服务 {self.base_url}: {e}") from e
        if resp.status_code != 200:
            raise JinaError(f"jina 服务返回 {resp.status_code}")
        return resp.json()

    # -- the media placeholder --------------------------------------------

    def media_marker(self, refresh: bool = False) -> str:
        """The placeholder the server uses for one piece of media.

        The server generates it at startup -- a random token, unless
        LLAMA_MEDIA_MARKER pins one -- so it has to be read back rather than
        assumed.

        The value is cached, because it is asked for once per picture and
        the server is not restarted often. But it does change when the
        server does restart, and a stale one makes every picture fail with
        "number of bitmaps (1) does not match number of markers (0)" --
        so a failed encode clears it and the next attempt asks again.
        """
        if self._marker is None or refresh:
            with self._marker_lock:
                if self._marker is None or refresh:
                    props = self._get("/props")
                    marker = props.get("media_marker")
                    if not marker:
                        raise JinaError("jina 服务没有返回 media_marker")
                    self._marker = marker
        return self._marker

    # -- encoding ----------------------------------------------------------

    def embed_texts(self, texts: list[str], prefix: str = "") -> list[list[float]]:
        """Encode text. Returns one vector per input, in order."""
        if not texts:
            return []
        payload = {"input": [f"{prefix}{t}" for t in texts]}
        out = self._post("/v1/embeddings", payload)
        data = out.get("data") if isinstance(out, dict) else None
        if not isinstance(data, list) or len(data) != len(texts):
            raise JinaError(f"jina 返回了 {len(data) if isinstance(data, list) else '?'} 条向量，期望 {len(texts)} 条")
        by_index = {item.get("index", i): item.get("embedding") for i, item in enumerate(data)}
        vectors = []
        for i in range(len(texts)):
            vec = by_index.get(i)
            if not vec:
                raise JinaError(f"jina 返回的第 {i} 条向量为空")
            vectors.append([float(x) for x in vec])
        return vectors

    def embed_text(self, text: str, prefix: str = "") -> list[float]:
        return self.embed_texts([text], prefix=prefix)[0]

    def embed_image_bytes(self, data: bytes) -> list[float]:
        """Encode one picture given its raw bytes."""
        marker = self.media_marker()
        payload = {
            "content": [{
                "prompt_string": marker,
                "multimodal_data": [base64.b64encode(data).decode("ascii")],
            }]
        }
        try:
            out = self._post("/embeddings", payload)
        except JinaError as e:
            # The server may have been restarted and invented a new
            # placeholder; the old one no longer matches. Ask again and
            # try once more.
            if "does not match number of markers" not in str(e):
                raise
            log.info("jina 服务换了媒体占位符，重新读取后重试")
            marker = self.media_marker(refresh=True)
            payload["content"][0]["prompt_string"] = marker
            out = self._post("/embeddings", payload)
        # The non-OpenAI endpoint answers with a list of results, each
        # holding a list of vectors (one per embedding row).
        try:
            rows = out[0]["embedding"]
            vector = rows[0] if rows and isinstance(rows[0], list) else rows
        except (KeyError, IndexError, TypeError) as e:
            raise JinaError(f"无法解析 jina 的图片向量: {str(out)[:200]}") from e
        if not vector:
            raise JinaError("jina 返回了空向量")
        return [float(x) for x in vector]

    def embed_image(self, path: Path | str) -> list[float]:
        p = Path(path)
        try:
            data = p.read_bytes()
        except OSError as e:
            raise JinaError(f"读取图片失败 {p}: {e}") from e
        return self.embed_image_bytes(data)

    # -- health ------------------------------------------------------------

    def available(self) -> bool:
        """Whether the service answers at all."""
        try:
            self._get("/props")
            return True
        except JinaError:
            return False

    def supports_images(self) -> bool:
        """Whether the service was started with a vision projector."""
        try:
            props = self._get("/props")
        except JinaError:
            return False
        modalities = props.get("modalities") or {}
        return bool(modalities.get("vision"))


_embedder: Optional[JinaEmbedder] = None
_embedder_lock = threading.Lock()


def get_image_embedder() -> JinaEmbedder:
    """The process-wide client, pointed at IMAGE_EMBEDDING_BASE_URL."""
    global _embedder
    with _embedder_lock:
        if _embedder is None:
            _embedder = JinaEmbedder(
                base_url=os.getenv("IMAGE_EMBEDDING_BASE_URL", DEFAULT_BASE_URL),
                timeout=float(os.getenv("IMAGE_EMBEDDING_TIMEOUT", DEFAULT_TIMEOUT)),
            )
        return _embedder


def is_image_file(path: Path | str) -> bool:
    return Path(path).suffix.lower() in IMAGE_EXTENSIONS


# The first bytes of the formats we accept. A file called .jpg is not
# always a picture: a download that failed leaves an HTML page under a
# name that says otherwise, and half-written uploads look like one.
IMAGE_MAGIC = (
    b"\xff\xd8\xff",          # jpeg
    b"\x89PNG\r\n\x1a\n",    # png
    b"GIF8",                     # gif
    b"RIFF",                     # webp (the WEBP tag is at offset 8)
    b"BM",                       # bmp
    b"II*\x00",                 # tiff, little endian
    b"MM\x00*",                 # tiff, big endian
)


def looks_like_an_image(path: Path | str) -> bool:
    """Whether the content of the file really is a picture.

    The name says one thing and the bytes another; encoding a file that
    is not a picture costs three seconds of the big cores and comes back
    as an error. Reading sixteen bytes is much cheaper than that.
    """
    try:
        with open(path, "rb") as f:
            head = f.read(16)
    except OSError:
        return False
    if not head:
        return False
    if head.startswith(b"RIFF") and head[8:12] == b"WEBP":
        return True
    # HEIC and the other ISO-BMFF formats: "....ftyp<type>"
    if len(head) >= 12 and head[4:8] == b"ftyp":
        return True
    return any(head.startswith(magic) for magic in IMAGE_MAGIC)
