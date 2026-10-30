import os
import shutil
import tempfile
import time
import zipfile
from datetime import datetime
from pathlib import Path
from typing import Optional

from fastapi import APIRouter, HTTPException, UploadFile, File, Request
from pydantic import BaseModel
from fastapi.responses import FileResponse, Response, StreamingResponse
from starlette.background import BackgroundTask
from ..deps import (
    state, get_storage_root, resolve_under_root, PathOutsideRoot,
    PathNotAllowed, check_access, visible_children, current_user,
)
from ..models import (
    BrowseResult,
    CreateFolderRequest,
    DeleteRequest,
    FileNode,
    FileOperation,
    MoveRequest,
)
from .. import uploads
from ..storage_stats import dir_size, disk_info, invalidate as invalidate_sizes
from ..trash import TrashError, TrashService

fs = APIRouter()

# Preview reads the whole file to slice it by character offset, so this bounds
# how large a file may be before preview refuses it outright.
MAX_PREVIEW_BYTES = 10 * 1024 * 1024


def _get_mime_type(path: Path) -> str:
    ext = path.suffix.lower()
    mime_types = {
        ".txt": "text/plain",
        ".md": "text/markdown",
        ".json": "application/json",
        ".xml": "application/xml",
        ".html": "text/html",
        ".css": "text/css",
        ".js": "application/javascript",
        ".ts": "application/typescript",
        ".py": "text/x-python",
        ".rs": "text/x-rust",
        ".go": "text/x-go",
        ".java": "text/x-java",
        ".c": "text/x-c",
        ".cpp": "text/x-c++",
        ".h": "text/x-c-header",
        ".jpg": "image/jpeg",
        ".jpeg": "image/jpeg",
        ".png": "image/png",
        ".gif": "image/gif",
        ".bmp": "image/bmp",
        ".webp": "image/webp",
        ".svg": "image/svg+xml",
        ".ico": "image/x-icon",
        ".mp3": "audio/mpeg",
        ".wav": "audio/wav",
        ".ogg": "audio/ogg",
        ".mp4": "video/mp4",
        ".avi": "video/x-msvideo",
        ".mkv": "video/x-matroska",
        ".mov": "video/quicktime",
        ".pdf": "application/pdf",
        ".zip": "application/zip",
        ".tar": "application/x-tar",
        ".gz": "application/gzip",
        ".7z": "application/x-7z-compressed",
        ".rar": "application/vnd.rar",
        ".doc": "application/msword",
        ".docx": "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        ".xls": "application/vnd.ms-excel",
        ".xlsx": "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        ".ppt": "application/vnd.ms-powerpoint",
        ".pptx": "application/vnd.openxmlformats-officedocument.presentationml.presentation",
    }
    return mime_types.get(ext, "application/octet-stream")


def _safe_path(base: str, target: str) -> Path:
    base_path = Path(base).resolve()
    raw = Path(target)
    # A relative path is taken from the storage root, not from
    # wherever the server happens to be running from -- the two are not
    # the same place, and a path that worked in one did not in the
    # other.
    target_path = (raw.resolve() if raw.is_absolute() else (base_path / raw).resolve())
    if not target_path.is_relative_to(base_path):
        raise HTTPException(status_code=400, detail="Path outside storage root")
    # The same permission check the other routes go through, so a path
    # cannot be reached by going around it.
    check_access(target_path, "read")
    return target_path


def _path_to_filenode(path: Path) -> FileNode:
    import datetime
    stat = path.stat()
    modified_dt = datetime.datetime.fromtimestamp(stat.st_mtime)
    created_dt = datetime.datetime.fromtimestamp(stat.st_ctime)
    return FileNode(
        name=path.name,
        path=str(path),
        is_dir=path.is_dir(),
        # A directory's size is the sum of everything under it, which
        # is what a user asking "how big is this" means. It is cached,
        # because walking every subtree on every listing is not free.
        size=dir_size(path),
        modified=modified_dt.strftime("%Y-%m-%dT%H:%M:%S"),
        created=created_dt.strftime("%Y-%m-%dT%H:%M:%S"),
        extension=path.suffix.lower(),
        mime_type=_get_mime_type(path),
    )


@fs.get("/browse")
def browse(path: Optional[str] = None) -> BrowseResult:
    storage_path = str(get_storage_root())
    if not path:
        target = Path(storage_path)
    else:
        target = _safe_path(storage_path, path)

    if not target.exists():
        raise HTTPException(status_code=404, detail="Path not found")
    if not target.is_dir():
        raise HTTPException(status_code=400, detail="Path is not a directory")

    items = []
    dirs_count = 0
    files_count = 0

    entries = [
        e for e in sorted(target.iterdir(), key=lambda x: (not x.is_dir(), x.name.lower()))
        if not e.name.startswith('.')
    ]
    # A user who may reach one folder should not see the names of the
    # others beside it.
    entries = visible_children(target, entries)

    for entry in entries:
        try:
            node = _path_to_filenode(entry)
            items.append(node)
            if node.is_dir:
                dirs_count += 1
            else:
                files_count += 1
        except (PermissionError, OSError):
            continue

    parent = str(target.parent) if target != Path(storage_path) else None

    # A key for the little pictures of this folder, so the browser can
    # show them without sending a header with every one.
    from .thumbnails import issue_directory_token

    # Which of these the person has starred. Sent with the listing so
    # the stars are right without a second question.
    favorite_paths = []
    try:
        from ..deps import current_user

        user = current_user()
        if user is not None:
            starred = state.get_favorite_store().paths_for(
                str(user.get("id") or user.get("username") or "")
            )
            root_path = get_storage_root()
            favorite_paths = [
                i.path for i in items
                if str(Path(i.path).relative_to(root_path)) in starred
            ]
    except Exception:
        favorite_paths = []

    return BrowseResult(
        current_path=str(target),
        parent_path=parent,
        items=items,
        thumbnail_token=issue_directory_token(str(target))["token"],
        favorite_paths=favorite_paths,
        total_count=len(items),
        dirs_count=dirs_count,
        files_count=files_count,
        disk=disk_info(Path(storage_path)),
    )


@fs.post("/create_folder")
def create_folder(req: CreateFolderRequest) -> FileOperation:
    storage_path = str(get_storage_root())
    target = _safe_path(storage_path, Path(req.path) / req.name)
    check_access(target, "write")

    try:
        target.mkdir(parents=True, exist_ok=False)
        invalidate_sizes(target.parent)
        return FileOperation(success=True, message="Folder created", path=str(target))
    except FileExistsError:
        raise HTTPException(status_code=409, detail="Folder already exists")
    except Exception as e:
        raise HTTPException(status_code=500, detail=str(e))


@fs.post("/move")
def move(req: MoveRequest) -> FileOperation:
    storage_path = str(get_storage_root())
    src = _safe_path(storage_path, req.src)
    dest = _safe_path(storage_path, req.dest)
    check_access(src, "write")
    check_access(dest, "write")

    if not src.exists():
        raise HTTPException(status_code=404, detail="Source not found")

    try:
        shutil.move(str(src), str(dest))
        invalidate_sizes(src)
        invalidate_sizes(dest)
        return FileOperation(success=True, message="File moved", path=str(dest))
    except Exception as e:
        raise HTTPException(status_code=500, detail=str(e))


@fs.post("/copy")
def copy(req: MoveRequest) -> FileOperation:
    storage_path = str(get_storage_root())
    src = _safe_path(storage_path, req.src)
    dest = _safe_path(storage_path, req.dest)
    check_access(dest, "write")

    if not src.exists():
        raise HTTPException(status_code=404, detail="Source not found")

    try:
        if src.is_dir():
            shutil.copytree(str(src), str(dest), dirs_exist_ok=False)
        else:
            shutil.copy2(str(src), str(dest))
        invalidate_sizes(dest)
        return FileOperation(success=True, message="File copied", path=str(dest))
    except Exception as e:
        raise HTTPException(status_code=500, detail=str(e))


def _trash_service() -> TrashService:
    return TrashService(Path(get_storage_root()))


class BatchRequest(BaseModel):
    action: str                      # delete | move | copy
    paths: list[str]
    dest: str = ""                   # target directory, for move and copy
    overwrite: str = "never"         # never | replace | rename, for copy


@fs.post("/batch")
def batch(req: BatchRequest) -> dict:
    """Do the same thing to a lot of files, in one request.

    The browser used to send a request per file, which for a thousand
    files is a thousand round trips and a thousand chances to lose
    track of where it got to. One request also means one decision
    about what to do, so the whole batch is handled the same way.

    Each file is reported separately: one that cannot be moved does not
    stop the others, and the caller is told which ones failed and
    why.
    """
    if req.action not in ("delete", "move", "copy"):
        raise HTTPException(status_code=400, detail="Unknown action")
    if not req.paths:
        raise HTTPException(status_code=400, detail="没有要处理的文件")
    if len(req.paths) > 5000:
        raise HTTPException(status_code=400, detail="一次最多处理 5000 项")

    storage_path = str(get_storage_root())
    trash_svc = _trash_service()
    succeeded: list[str] = []
    failed: list[dict] = []

    for raw in req.paths:
        try:
            src = _safe_path(storage_path, raw)
            if req.action != "delete":
                check_access(src, "write")
        except HTTPException as e:
            failed.append({"path": raw, "error": str(e.detail)})
            continue
        except PathNotAllowed as e:
            failed.append({"path": raw, "error": str(e)})
            continue
        if not src.exists():
            failed.append({"path": raw, "error": "文件不存在"})
            continue

        try:
            if req.action == "delete":
                invalidate_sizes(src)
                trash_svc.trash(src)
            else:
                if not req.dest:
                    raise ValueError("缺少目标目录")
                dest_dir = _safe_path(storage_path, req.dest)
                if not dest_dir.is_dir():
                    raise ValueError("目标不是目录")
                dest = dest_dir / src.name
                if dest.exists() and req.action == "copy":
                    if req.overwrite == "never":
                        raise ValueError("目标已存在")
                    if req.overwrite == "rename":
                        dest = uploads._free_name(dest_dir, src.name)
                if req.action == "move":
                    if dest.exists():
                        raise ValueError("目标已存在")
                    shutil.move(str(src), str(dest))
                else:
                    if src.is_dir():
                        shutil.copytree(str(src), str(dest), dirs_exist_ok=False)
                    else:
                        shutil.copy2(str(src), str(dest))
                invalidate_sizes(dest)
            succeeded.append(str(src))
        except Exception as e:
            failed.append({"path": raw, "error": f"{type(e).__name__}: {str(e)[:160]}"})

    return {
        "success": not failed,
        "action": req.action,
        "succeeded": succeeded,
        "failed": failed,
        "count": len(succeeded),
        "failed_count": len(failed),
    }


@fs.post("/delete")
def delete(req: DeleteRequest) -> FileOperation:
    """Delete a file or directory.

    By default it goes to the recycle bin, where it can be put back.
    The caller has to ask for it to be destroyed outright.
    """
    storage_path = str(get_storage_root())
    target = _safe_path(storage_path, req.path)
    check_access(target, "write")

    if not target.exists():
        raise HTTPException(status_code=404, detail="Path not found")

    if getattr(req, "permanent", False):
        try:
            if target.is_dir():
                shutil.rmtree(str(target))
            else:
                target.unlink()
            return FileOperation(success=True, message="已永久删除", path=str(target))
        except Exception as e:
            raise HTTPException(status_code=500, detail=str(e))

    try:
        entry = _trash_service().trash(target)
    except TrashError as e:
        raise HTTPException(status_code=400, detail=str(e))
    except Exception as e:
        raise HTTPException(status_code=500, detail=str(e))
    return FileOperation(
        success=True,
        message="已移到回收站，可在回收站中还原",
        path=str(target),
    )


@fs.get("/trash")
def list_trash() -> dict:
    """Everything currently in the recycle bin, newest first."""
    svc = _trash_service()
    entries = svc.list()
    return {
        "entries": entries,
        "count": len(entries),
        "size": sum(e.get("size", 0) for e in entries),
    }


@fs.post("/trash/{entry_id}/restore")
def restore_trash(entry_id: str) -> FileOperation:
    """Put a deleted file back where it came from."""
    try:
        result = _trash_service().restore(entry_id)
    except TrashError as e:
        raise HTTPException(status_code=400, detail=str(e))
    except Exception as e:
        raise HTTPException(status_code=500, detail=str(e))
    message = "已还原"
    if result.get("renamed"):
        message = f"原位置已有同名文件，已还原为 {result['path'].split('/')[-1]}"
    return FileOperation(success=True, message=message, path=result["path"])


@fs.delete("/trash/{entry_id}")
def purge_trash(entry_id: str) -> FileOperation:
    """Destroy one entry for good."""
    try:
        result = _trash_service().purge(entry_id)
    except TrashError as e:
        raise HTTPException(status_code=400, detail=str(e))
    except Exception as e:
        raise HTTPException(status_code=500, detail=str(e))
    return FileOperation(
        success=True, message=f"已永久删除 {result.get('name', '')}", path=""
    )


@fs.post("/trash/empty")
def empty_trash() -> FileOperation:
    """Destroy everything in the bin."""
    try:
        result = _trash_service().empty()
    except Exception as e:
        raise HTTPException(status_code=500, detail=str(e))
    return FileOperation(
        success=True, message=f"已清空回收站（{result.get('removed', 0)} 项）", path=""
    )


@fs.get("/info")
def get_info(path: str) -> FileNode:
    storage_path = str(get_storage_root())
    target = _safe_path(storage_path, path)

    if not target.exists():
        raise HTTPException(status_code=404, detail="Path not found")

    return _path_to_filenode(target)


@fs.get("/tree")
def get_tree(path: Optional[str] = None, depth: int = 2) -> dict:
    storage_path = str(get_storage_root())
    if not path:
        target = Path(storage_path)
    else:
        target = _safe_path(storage_path, path)

    def build_tree(p: Path, current_depth: int) -> dict:
        if current_depth > depth:
            return {"name": p.name, "path": str(p), "is_dir": True, "children": []}
        if not p.is_dir():
            # Leaves carry the metadata a preview needs; the UI opens tree files
            # directly instead of browsing into them.
            try:
                st = p.stat()
                return {
                    "name": p.name,
                    "path": str(p),
                    "is_dir": False,
                    "size": st.st_size,
                    "modified": datetime.fromtimestamp(st.st_mtime).isoformat(),
                }
            except (PermissionError, OSError):
                return {"name": p.name, "path": str(p), "is_dir": False}
        children = []
        try:
            entries = [
                c for c in sorted(p.iterdir(), key=lambda x: (not x.is_dir(), x.name.lower()))
                if not c.name.startswith('.')
            ]
            # The tree shows the same things the listing does: only what
            # the caller may reach.
            entries = visible_children(p, entries)
            for child in entries:
                try:
                    children.append(build_tree(child, current_depth + 1))
                except (PermissionError, OSError):
                    continue
        except (PermissionError, OSError):
            pass
        return {"name": p.name, "path": str(p), "is_dir": True, "children": children}

    return build_tree(target, 0)


@fs.get("/download")
def download_file(path: str, request: Request = None):
    """Download a file, in parts when the client asks for them.

    A range request lets a browser resume a download that was cut off
    and lets it start playing a large file before it has all arrived.
    Without it the whole file has to cross the wire before anything
    happens, and the browser has to hold all of it.
    """
    try:
        target = resolve_under_root(path)
    except PathOutsideRoot:
        raise HTTPException(status_code=400, detail="Path outside storage root")
    if not target.exists() or not target.is_file():
        raise HTTPException(status_code=404, detail="File not found")

    size = target.stat().st_size
    range_header = request.headers.get("range") if request else None
    if not range_header:
        return FileResponse(
            str(target),
            filename=target.name,
            media_type=_get_mime_type(target),
            headers={"Accept-Ranges": "bytes"},
        )

    start, end = _parse_range(range_header, size)
    if start is None:
        # An unsatisfiable range is answered with the size, so the
        # client knows what it asked for was out of bounds.
        return Response(
            status_code=416,
            headers={"Content-Range": f"bytes */{size}"},
        )
    length = end - start + 1
    return StreamingResponse(
        _range_stream(target, start, length),
        status_code=206,
        media_type=_get_mime_type(target),
        headers={
            "Content-Range": f"bytes {start}-{end}/{size}",
            "Accept-Ranges": "bytes",
            "Content-Length": str(length),
            "Content-Disposition": _content_disposition(target.name),
        },
    )


def _range_stream(path: Path, start: int, length: int):
    """Yield just the requested bytes of a file."""
    with open(path, "rb") as f:
        f.seek(start)
        remaining = length
        while remaining > 0:
            chunk = f.read(min(1024 * 1024, remaining))
            if not chunk:
                break
            remaining -= len(chunk)
            yield chunk


def _content_disposition(filename: str) -> str:
    """A Content-Disposition header that survives a non-ASCII name."""
    from urllib.parse import quote

    ascii_name = filename.encode("ascii", "ignore").decode() or "download"
    return f"attachment; filename=\"{ascii_name}\"; filename*=UTF-8''{quote(filename)}"


def _parse_range(header: str, size: int) -> tuple[Optional[int], int]:
    """Read a byte range header. Returns (start, end), start None if bad."""
    try:
        spec = header.split("=", 1)[1].split(",")[0].strip()
        first, _, last = spec.partition("-")
        if not first:
            # "bytes=-500" means the last 500 bytes.
            length = int(last)
            if length <= 0:
                return None, 0
            start = max(0, size - length)
            return start, size - 1
        start = int(first)
        end = int(last) if last else size - 1
    except (ValueError, IndexError):
        return None, 0
    if start >= size or start < 0:
        return None, 0
    end = min(end, size - 1)
    if end < start:
        return None, 0
    return start, end


@fs.get("/download-zip")
def download_zip(path: str):
    """Download a directory as a zip archive.

    A NAS is mostly directories, and a browser cannot download one.
    The archive is built into a temporary file and streamed, so a
    large tree does not have to fit in memory.
    """
    try:
        target = resolve_under_root(path)
    except PathOutsideRoot:
        raise HTTPException(status_code=400, detail="Path outside storage root")
    if not target.exists():
        raise HTTPException(status_code=404, detail="Path not found")
    if not target.is_dir():
        raise HTTPException(status_code=400, detail="Not a directory")

    tmp = tempfile.NamedTemporaryFile(suffix=".zip", delete=False)
    tmp_path = Path(tmp.name)
    tmp.close()
    try:
        with zipfile.ZipFile(tmp_path, "w", zipfile.ZIP_DEFLATED, allowZip64=True) as zf:
            base = target.parent
            for root, dirs, files in os.walk(target):
                dirs.sort()
                for name in sorted(files):
                    full = Path(root) / name
                    try:
                        zf.write(full, full.relative_to(base))
                    except OSError:
                        # A file that vanished mid-walk, or one the
                        # process may not read, should not lose the
                        # whole archive.
                        continue
    except Exception as e:
        tmp_path.unlink(missing_ok=True)
        raise HTTPException(status_code=500, detail=f"打包失败: {e}")

    archive_name = f"{target.name}.zip"
    return FileResponse(
        str(tmp_path),
        filename=archive_name,
        media_type="application/zip",
        background=BackgroundTask(_remove_file, tmp_path),
    )


def _remove_file(path: Path):
    try:
        path.unlink(missing_ok=True)
    except OSError:
        pass


@fs.post("/upload")
async def upload_files(path: str = "", files: list[UploadFile] = File(...),
                       overwrite: str = "never"):
    """Upload files in one request.

    Kept for small files and for clients that cannot do better. A name
    that is already taken is refused rather than replaced, because
    replacing it is how a day's work disappears without a word.
    """
    try:
        target_dir = resolve_under_root(path, "write")
    except PathOutsideRoot:
        raise HTTPException(status_code=400, detail="Path outside storage root")
    if not target_dir.is_dir():
        raise HTTPException(status_code=400, detail="Destination is not a directory")

    uploaded = []
    for f in files:
        raw_filename = f.filename or ""
        filename = Path(raw_filename.replace("\\", "/")).name
        if (not filename or filename in (".", "..")
                or "/" in raw_filename or "\\" in raw_filename):
            raise HTTPException(status_code=400, detail="Invalid upload filename")
        dest = (target_dir / filename).resolve()
        if not dest.is_relative_to(target_dir.resolve()):
            raise HTTPException(status_code=400, detail="Upload destination outside storage root")
        if dest.exists() and overwrite in ("replace", "rename"):
            # What is there now is kept, so replacing it can be undone.
            state.get_version_store().snapshot(dest, note="被上传的新文件覆盖")
        if dest.exists() and overwrite == "never":
            st = dest.stat()
            raise HTTPException(
                status_code=409,
                detail={
                    "message": f"{filename} 已经存在",
                    "name": filename,
                    "size": st.st_size,
                    "modified": time.strftime(
                        "%Y-%m-%d %H:%M:%S", time.localtime(st.st_mtime)
                    ),
                },
            )
        if dest.exists() and overwrite == "rename":
            dest = uploads._free_name(target_dir, filename)
        with open(dest, "wb") as out:
            while chunk := await f.read(1024 * 1024):
                out.write(chunk)
        uploaded.append(dest.name)
        try:
            state.get_upload_log().record(
                path=str(dest),
                size=dest.stat().st_size,
                name=dest.name,
                user=current_user(),
            )
        except Exception:
            # The file is on disk and the upload succeeded; a log that
            # cannot be written must not turn that into a failure.
            pass
    return {"success": True, "uploaded": uploaded, "count": len(uploaded)}


def _upload_manager() -> uploads.UploadManager:
    return uploads.UploadManager(Path(get_storage_root()))


@fs.post("/upload/init")
def upload_init(path: str = "", name: str = "", size: int = 0, total_chunks: int = 0, rel_dir: str = ""):
    """Begin an upload and ask what is already at the destination.

    `rel_dir` is where inside `path` the file goes -- the inside of a
    folder that was dropped onto the browser, so the shape of that
    folder survives the trip.
    """
    try:
        target_dir = resolve_under_root(path, "write")
    except PathOutsideRoot:
        raise HTTPException(status_code=400, detail="Path outside storage root")
    if not target_dir.is_dir():
        raise HTTPException(status_code=400, detail="Destination is not a directory")
    filename = Path((name or "").replace("\\", "/")).name
    if not filename or filename in (".", ".."):
        raise HTTPException(status_code=400, detail="Invalid upload filename")
    if total_chunks <= 0 or total_chunks > 1_000_000:
        raise HTTPException(status_code=400, detail="Invalid chunk count")
    try:
        sub = uploads.safe_subdir(target_dir, rel_dir)
    except uploads.UploadError as e:
        raise HTTPException(status_code=400, detail=str(e))
    # The file's own destination decides the permission, not the folder
    # it was dropped into -- a read-only subfolder stays read-only.
    check_access(sub / filename, "write")
    try:
        return _upload_manager().init(target_dir, filename, size, total_chunks, rel_dir)
    except uploads.UploadError as e:
        raise HTTPException(status_code=400, detail=str(e))


@fs.post("/upload/chunk")
async def upload_chunk(upload_id: str, index: int = 0, request: Request = None):
    """Receive one piece of an upload."""
    body = await request.body()
    try:
        return _upload_manager().write_chunk(upload_id, index, body)
    except uploads.UploadError as e:
        raise HTTPException(status_code=400, detail=str(e))


@fs.get("/upload/status")
def upload_status(upload_id: str):
    """Which pieces have arrived, so a dropped client can pick up."""
    try:
        return _upload_manager().status(upload_id)
    except uploads.UploadError as e:
        raise HTTPException(status_code=404, detail=str(e))


@fs.post("/upload/complete")
def upload_complete(upload_id: str, overwrite: str = "never"):
    """Put the file in place."""
    try:
        result = _upload_manager().complete(upload_id, overwrite)
        try:
            state.get_upload_log().record(
                path=str(result.get("path", "")),
                size=int(result.get("size", 0) or 0),
                name=str(result.get("name", "")),
                user=current_user(),
            )
        except Exception:
            pass
        return result
    except uploads.UploadConflict as e:
        raise HTTPException(
            status_code=409,
            detail={"message": str(e), "existing": e.existing},
        )
    except uploads.UploadError as e:
        raise HTTPException(status_code=400, detail=str(e))


@fs.delete("/upload")
def upload_abandon(upload_id: str):
    """Throw away an upload that will not be finished."""
    try:
        return _upload_manager().abandon(upload_id)
    except uploads.UploadError as e:
        raise HTTPException(status_code=400, detail=str(e))


@fs.get("/preview")
def preview_file(path: str):
    """A file rendered for reading in the browser.

    Plain text is returned as text, a document as the text inside it,
    and a spreadsheet as a table. The point is that a Word file or a
    budget can be read without downloading it and without opening
    another program.
    """
    try:
        target = resolve_under_root(path)
    except PathOutsideRoot:
        raise HTTPException(status_code=400, detail="Path outside storage root")
    if not target.exists() or not target.is_file():
        raise HTTPException(status_code=404, detail="File not found")

    suffix = target.suffix.lower()
    size = target.stat().st_size
    if size > 64 * 1024 * 1024:
        raise HTTPException(status_code=413, detail="文件太大，无法预览")

    kind = "text"
    if suffix in (".docx", ".xlsx", ".pptx", ".pdf"):
        from ..doc_extract import ExtractionError, extract_text

        try:
            content = extract_text(target)
        except ExtractionError as e:
            raise HTTPException(status_code=422, detail=str(e))
        kind = "document"
    else:
        try:
            raw = target.read_bytes()[:MAX_PREVIEW_BYTES]
        except OSError as e:
            raise HTTPException(status_code=500, detail=str(e))
        content = raw.decode("utf-8", errors="replace")

    # A spreadsheet is a table, and reading it as a wall of tab-separated
    # text loses the shape entirely.
    rows = None
    if suffix == ".xlsx":
        try:
            from ..doc_extract import _extract_xlsx

            rows = _xlsx_rows(target)
        except Exception:
            rows = None

    return {
        "name": target.name,
        "kind": kind,
        "content": content,
        "truncated": len(content) >= MAX_PREVIEW_BYTES,
        "rows": rows,
    }


def _xlsx_rows(path: Path) -> list[list[str]]:
    """The first sheet of a workbook as a grid."""
    import openpyxl

    workbook = openpyxl.load_workbook(str(path), read_only=True, data_only=True)
    try:
        sheet = workbook.worksheets[0]
        out = []
        for row in sheet.iter_rows(values_only=True):
            out.append(["" if c is None else str(c) for c in row])
            if len(out) >= 200:
                break
        return out
    finally:
        try:
            workbook.close()
        except Exception:
            pass


class StreamTokenRequest(BaseModel):
    path: str
    # How long the token is good for. A player asks for minutes; a
    # download over a slow line asks for longer.
    ttl: int = 600
    # A directory is packed into an archive rather than served, so a
    # token for one opens the archive and not the directory.
    archive: bool = False


# A token is a key to one file. It is not a session and it does not
# grant anything else, but it should not outlive its welcome either.
MAX_STREAM_TOKEN_TTL = 3600


@fs.post("/stream-token")
def stream_token(req: StreamTokenRequest):
    """A short-lived URL for a media file.

    A <video> tag cannot send an Authorization header, and neither can
    the browser's own download machinery. This issues a token that is
    good for one file and a while, so the file can be streamed instead
    of the browser holding all of it in memory.
    """
    try:
        target = resolve_under_root(req.path)
    except PathOutsideRoot:
        raise HTTPException(status_code=400, detail="Path outside storage root")
    if not target.exists():
        raise HTTPException(status_code=404, detail="File not found")
    if req.archive:
        if not target.is_dir():
            raise HTTPException(status_code=400, detail="Not a directory")
    elif not target.is_file():
        raise HTTPException(status_code=404, detail="File not found")

    from ..routers.auth import issue_stream_token

    ttl = max(60, min(int(req.ttl or 600), MAX_STREAM_TOKEN_TTL))
    issued = issue_stream_token(str(target), ttl=ttl)
    from urllib.parse import quote

    endpoint = "download-zip" if req.archive else "download"
    return {
        "token": issued["token"],
        "expires_at": issued["expires_at"],
        "url": f"/api/fs/{endpoint}?path={quote(req.path)}&st={issued['token']}",
    }


@fs.get("/thumbnail")
def thumbnail(path: str, size: int = 256):
    """A little picture of a big one.

    Made once and kept, so scrolling a folder of photographs does not
    make the machine work hard. A file that is not a picture gets a
    404, and the browser shows the icon it would have shown anyway.
    """
    from ..thumbnails import DEFAULT_SIZE, ThumbnailStore

    try:
        target = resolve_under_root(path)
    except PathOutsideRoot:
        raise HTTPException(status_code=400, detail="Path outside storage root")
    if not target.exists() or not target.is_file():
        raise HTTPException(status_code=404, detail="File not found")

    size = max(32, min(int(size or DEFAULT_SIZE), 1024))
    data = ThumbnailStore(get_storage_root()).get(target, size)
    if not data:
        raise HTTPException(status_code=404, detail="No thumbnail")
    return Response(content=data, media_type="image/jpeg", headers={"Cache-Control": "private, max-age=86400"})


@fs.get("/duplicates")
def find_duplicates(limit: int = 200) -> dict:
    """The files that are on the machine more than once.

    The names do not have to match -- a photo off the phone and off
    the laptop are the same photo. They are found by what is in them.
    """
    from ..duplicates import DuplicateFinder

    return DuplicateFinder().find(limit=limit)


@fs.post("/duplicates/scan")
def scan_for_duplicates() -> dict:
    """Work out the hashes of files that do not have one yet."""
    from ..duplicates import DuplicateFinder

    return DuplicateFinder().scan_for_duplicates()


@fs.post("/duplicates/{digest}/clean")
def clean_duplicates(digest: str, keep: str = "") -> dict:
    """Put the extra copies of a file in the recycle bin.

    One copy is kept -- the one named, or else the oldest -- and the
    rest go to the bin rather than nowhere, so a mistake here is a
    thing that can be undone.
    """
    from ..duplicates import DuplicateFinder

    try:
        return DuplicateFinder().clean(digest, keep=keep)
    except ValueError as e:
        raise HTTPException(status_code=400, detail=str(e))


@fs.get("/versions")
def list_versions(path: str):
    """What this file used to contain."""
    try:
        target = resolve_under_root(path)
    except PathOutsideRoot:
        raise HTTPException(status_code=400, detail="Path outside storage root")
    store = state.get_version_store()
    return {"path": str(target), "versions": store.list(target)}


@fs.post("/versions/{version_id}/restore")
def restore_version(version_id: int, path: str = ""):
    """Put a version back."""
    from ..versions import VersionError

    try:
        target = resolve_under_root(path) if path else None
    except PathOutsideRoot:
        raise HTTPException(status_code=400, detail="Path outside storage root")
    try:
        result = state.get_version_store().restore(version_id, target)
    except VersionError as e:
        raise HTTPException(status_code=400, detail=str(e))
    invalidate_sizes(Path(result["path"]))
    return result


@fs.delete("/versions/{version_id}")
def delete_version(version_id: int):
    """Throw a version away."""
    if not state.get_version_store().delete(version_id):
        raise HTTPException(status_code=404, detail="找不到这个版本")
    return {"success": True}


@fs.get("/content")
def get_content(path: str, limit: int = 50000, offset: int = 0):
    """Return one slice of a text file.

    Long documents (a novel is 1.7MB) are served in `limit`-sized character
    segments via `offset`, because reading the whole thing into a modal at once
    both stalls the browser and shows only a fraction of the text. Offsets count
    characters, not bytes, so multi-byte CJK text stays aligned between calls.
    """
    try:
        target = resolve_under_root(path)
    except PathOutsideRoot:
        raise HTTPException(status_code=400, detail="Path outside storage root")
    if not target.exists() or not target.is_file():
        raise HTTPException(status_code=404, detail="File not found")

    size = target.stat().st_size
    if size > MAX_PREVIEW_BYTES:
        raise HTTPException(
            status_code=413,
            detail=f"File too large to preview (max {MAX_PREVIEW_BYTES // 1024 // 1024}MB)",
        )

    limit = max(1000, min(limit, 500_000))
    offset = max(0, offset)
    try:
        # Decoding errors are dropped rather than raised, so the character count
        # matches what the client actually receives.
        text = target.read_text(encoding="utf-8", errors="ignore")
    except (PermissionError, OSError, ValueError) as e:
        raise HTTPException(status_code=500, detail=str(e))

    total_chars = len(text)
    segment = text[offset: offset + limit]
    end = offset + len(segment)
    return {
        "path": str(target),
        "name": target.name,
        "content": segment,
        "offset": offset,
        "next_offset": end if end < total_chars else None,
        "total_chars": total_chars,
        "size": size,
        "mime_type": _get_mime_type(target),
    }
