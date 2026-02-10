import os
import shutil
from .base import StorageDriver
from ..core.config import settings
from typing import Optional, Dict, Any

class PathStorageDriver(StorageDriver):
    def save_file(self, content: bytes, relative_path: str) -> str:
        full_path = os.path.join(settings.UPLOAD_DIR, relative_path)
        os.makedirs(os.path.dirname(full_path), exist_ok=True)
        with open(full_path, "wb") as f:
            f.write(content)
        return full_path

    def create_folder(self, relative_path: str) -> None:
        full_path = os.path.join(settings.UPLOAD_DIR, relative_path)
        os.makedirs(full_path, exist_ok=True)

    def delete_item(self, relative_path: str) -> None:
        full_path = os.path.join(settings.UPLOAD_DIR, relative_path)
        if os.path.exists(full_path):
            if os.path.isdir(full_path):
                shutil.rmtree(full_path)
            else:
                os.remove(full_path)

    def move_item(self, old_path: str, new_path: str) -> None:
        old_full = os.path.join(settings.UPLOAD_DIR, old_path)
        new_full = os.path.join(settings.UPLOAD_DIR, new_path)
        if os.path.exists(old_full):
            os.makedirs(os.path.dirname(new_full), exist_ok=True)
            shutil.move(old_full, new_full)

    def get_physical_path(self, relative_path: str, file_id: Optional[int] = None) -> str:
        return os.path.join(settings.UPLOAD_DIR, relative_path)

    def list_contents(self, prefix: str) -> Dict[str, Any]:
        """Raw filesystem scan (for DB-less mode)."""
        target_dir = os.path.join(settings.UPLOAD_DIR, prefix)
        if not os.path.exists(target_dir):
            return {"folders": [], "files": []}
        
        folders = []
        files = []
        for entry in os.scandir(target_dir):
            if entry.is_dir():
                folders.append(entry.name)
            else:
                # Basic metadata for DB-less mode
                stat = entry.stat()
                files.append({
                    "id": hash(entry.path), # Temp pseudo-id
                    "filename": os.path.join(prefix, entry.name).replace("\\", "/"),
                    "size": stat.st_size,
                    "upload_time": os.path.getmtime(entry.path),
                    "comment": "Auto-scanned"
                })
        return {"folders": folders, "files": files}
