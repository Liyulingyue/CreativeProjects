import hashlib
import json
import threading
import numpy as np
from pathlib import Path
from typing import Optional
from datetime import datetime


CACHE_DIR = Path(__file__).resolve().parent.parent.parent / "data" / "features"
CACHE_DIR.mkdir(parents=True, exist_ok=True)

_HASHES_FILE = CACHE_DIR / "hashes.json"
_EMBEDDINGS_DIR = CACHE_DIR / "embeddings"
_EMBEDDINGS_DIR.mkdir(exist_ok=True)
_EXIF_FILE = CACHE_DIR / "exif.json"


def _load_json(path: Path, default=None):
    if not path.exists():
        return default if default is not None else {}
    with open(path, "r", encoding="utf-8") as f:
        return json.load(f)


def _save_json(path: Path, data):
    with open(path, "w", encoding="utf-8") as f:
        json.dump(data, f, indent=2, ensure_ascii=False)


def _path_key(file_path: str, mtime: float) -> str:
    return hashlib.sha256(f"{file_path}:{mtime}".encode()).hexdigest()[:16]


class FeatureCache:
    def __init__(self):
        self._lock = threading.Lock()
        self._hashes: dict = _load_json(_HASHES_FILE, {})
        self._exif: dict = _load_json(_EXIF_FILE, {})

    def _flush_hashes(self):
        _save_json(_HASHES_FILE, self._hashes)

    def _flush_exif(self):
        _save_json(_EXIF_FILE, self._exif)

    def get_hash(self, file_path: str, mtime: float, hash_type: str) -> Optional[dict]:
        key = _path_key(file_path, mtime)
        bucket = self._hashes.get(key)
        if bucket and bucket.get("type") == f"hash_{hash_type}":
            return bucket.get("data")
        return None

    def set_hash(self, file_path: str, mtime: float, hash_type: str, signatures: dict):
        key = _path_key(file_path, mtime)
        with self._lock:
            self._hashes[key] = {
                "type": f"hash_{hash_type}",
                "file_path": file_path,
                "mtime": mtime,
                "data": signatures,
            }
            self._flush_hashes()

    def get_embedding(self, file_path: str, mtime: float, model_name: str) -> Optional[np.ndarray]:
        emb_path = _EMBEDDINGS_DIR / f"{_path_key(file_path, mtime)}_{model_name}.json"
        if not emb_path.exists():
            return None
        data = _load_json(emb_path)
        if data and data.get("model") == model_name:
            return np.array(data["embedding"], dtype=np.float32)
        return None

    def set_embedding(self, file_path: str, mtime: float, model_name: str, embedding: np.ndarray):
        key = _path_key(file_path, mtime)
        emb_path = _EMBEDDINGS_DIR / f"{key}_{model_name}.json"
        data = {
            "model": model_name,
            "file_path": file_path,
            "mtime": mtime,
            "embedding": embedding.astype(np.float32).tolist(),
        }
        with self._lock:
            _save_json(emb_path, data)

    def get_exif(self, file_path: str, mtime: float) -> Optional[dict]:
        key = _path_key(file_path, mtime)
        entry = self._exif.get(key)
        if entry:
            result = dict(entry.get("data", {}))
            if result.get("datetime") and isinstance(result["datetime"], str):
                result["datetime"] = datetime.fromisoformat(result["datetime"])
            return result
        return None

    def set_exif(self, file_path: str, mtime: float, feature: dict):
        key = _path_key(file_path, mtime)
        serializable = dict(feature)
        if serializable.get("datetime"):
            serializable["datetime"] = serializable["datetime"].isoformat()
        with self._lock:
            self._exif[key] = {
                "file_path": file_path,
                "mtime": mtime,
                "data": serializable,
            }
            self._flush_exif()

    def stats(self) -> dict:
        hash_types: dict[str, int] = {}
        for entry in self._hashes.values():
            t = entry.get("type", "unknown")
            hash_types[t] = hash_types.get(t, 0) + 1

        emb_types: dict[str, int] = {}
        for f in _EMBEDDINGS_DIR.glob("*.json"):
            model = f.stem.split("_", 1)[-1] if "_" in f.stem else "unknown"
            emb_types[f"emb_{model}"] = emb_types.get(f"emb_{model}", 0) + 1

        exif_count = len(self._exif)

        result = {}
        result.update(hash_types)
        result.update(emb_types)
        if exif_count:
            result["exif"] = exif_count
        return result

    def list_entries(self, feature_type: Optional[str] = None) -> list[dict]:
        entries = []
        if not feature_type or feature_type.startswith("hash_"):
            for key, entry in self._hashes.items():
                if feature_type and entry.get("type") != feature_type:
                    continue
                entries.append({
                    "cache_key": key,
                    "feature_type": entry.get("type", ""),
                    "file_path": entry.get("file_path", ""),
                    "mtime": entry.get("mtime", 0),
                    "data": entry.get("data", {}),
                })

        if not feature_type or feature_type.startswith("emb_"):
            for f in _EMBEDDINGS_DIR.glob("*.json"):
                data = _load_json(f)
                if not data:
                    continue
                model = data.get("model", "unknown")
                ft = f"emb_{model}"
                if feature_type and ft != feature_type:
                    continue
                entries.append({
                    "cache_key": f.stem,
                    "feature_type": ft,
                    "file_path": data.get("file_path", ""),
                    "mtime": data.get("mtime", 0),
                    "data": {"model": model, "dim": len(data.get("embedding", []))},
                })

        if not feature_type or feature_type == "exif":
            for key, entry in self._exif.items():
                if feature_type and "exif" != feature_type:
                    continue
                entries.append({
                    "cache_key": key,
                    "feature_type": "exif",
                    "file_path": entry.get("file_path", ""),
                    "mtime": entry.get("mtime", 0),
                    "data": entry.get("data", {}),
                })

        return entries

    def clear(self, feature_type: Optional[str] = None):
        with self._lock:
            if feature_type is None:
                self._hashes.clear()
                self._exif.clear()
                for f in _EMBEDDINGS_DIR.glob("*.json"):
                    f.unlink()
                self._flush_hashes()
                self._flush_exif()
            elif feature_type.startswith("hash_"):
                self._hashes = {
                    k: v for k, v in self._hashes.items() if v.get("type") != feature_type
                }
                self._flush_hashes()
            elif feature_type.startswith("emb_"):
                model = feature_type[4:]
                for f in list(_EMBEDDINGS_DIR.glob("*.json")):
                    if f.stem.endswith(f"_{model}"):
                        f.unlink()
            elif feature_type == "exif":
                self._exif.clear()
                self._flush_exif()

    def delete_entry(self, cache_key: str) -> bool:
        with self._lock:
            if cache_key in self._hashes:
                del self._hashes[cache_key]
                self._flush_hashes()
                return True
            if cache_key in self._exif:
                del self._exif[cache_key]
                self._flush_exif()
                return True
            emb_file = _EMBEDDINGS_DIR / f"{cache_key}.json"
            if emb_file.exists():
                emb_file.unlink()
                return True
        return False


cache = FeatureCache()
