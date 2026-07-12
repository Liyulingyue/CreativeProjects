import hashlib
import json
import os
import threading
import numpy as np
from pathlib import Path
from typing import Optional, Literal
from datetime import datetime


PROJECT_CACHE_DIR = Path(__file__).resolve().parent.parent.parent / "data" / "features"
PROJECT_CACHE_DIR.mkdir(parents=True, exist_ok=True)

FOLDER_CACHE_DIR_NAME = ".photoanalyzer"


def _load_json(path: Path, default=None):
    if not path.exists():
        return default if default is not None else {}
    with open(path, "r", encoding="utf-8") as f:
        return json.load(f)


def _save_json(path: Path, data):
    path.parent.mkdir(parents=True, exist_ok=True)
    with open(path, "w", encoding="utf-8") as f:
        json.dump(data, f, indent=2, ensure_ascii=False)


def _path_key(file_path: str, mtime: float) -> str:
    return hashlib.sha256(f"{file_path}:{mtime}".encode()).hexdigest()[:16]


def _folder_cache_dir(base_dir: Path) -> Path:
    return base_dir / FOLDER_CACHE_DIR_NAME


def _find_base_dir(file_path: str) -> Optional[Path]:
    p = Path(file_path).resolve()
    for parent in [p.parent] + list(p.parents):
        if (parent / FOLDER_CACHE_DIR_NAME).exists():
            return parent
    return p.parent


class FeatureCache:
    def __init__(self, mode: Literal["project", "folder"] = "project"):
        self._mode = mode
        self._lock = threading.Lock()
        self._hashes: dict = _load_json(PROJECT_CACHE_DIR / "hashes.json", {})
        self._exif: dict = _load_json(PROJECT_CACHE_DIR / "exif.json", {})

    @property
    def mode(self) -> str:
        return self._mode

    @mode.setter
    def mode(self, value: str):
        if value in ("project", "folder"):
            self._mode = value

    def _flush_hashes(self):
        _save_json(PROJECT_CACHE_DIR / "hashes.json", self._hashes)

    def _flush_exif(self):
        _save_json(PROJECT_CACHE_DIR / "exif.json", self._exif)

    def _resolve_path(self, file_path: str) -> tuple[str, Optional[Path]]:
        if self._mode == "project":
            return file_path, None
        base = _find_base_dir(file_path)
        try:
            rel = str(Path(file_path).resolve().relative_to(base))
        except ValueError:
            rel = file_path
        return rel, base

    def _folder_hashes_file(self, base: Path) -> Path:
        d = _folder_cache_dir(base)
        d.mkdir(parents=True, exist_ok=True)
        return d / "hashes.json"

    def _folder_exif_file(self, base: Path) -> Path:
        d = _folder_cache_dir(base)
        d.mkdir(parents=True, exist_ok=True)
        return d / "exif.json"

    def _folder_emb_dir(self, base: Path) -> Path:
        d = _folder_cache_dir(base) / "embeddings"
        d.mkdir(parents=True, exist_ok=True)
        return d

    def _read_folder_hashes(self, base: Path) -> dict:
        return _load_json(self._folder_hashes_file(base), {})

    def _write_folder_hashes(self, base: Path, data: dict):
        _save_json(self._folder_hashes_file(base), data)

    def _read_folder_exif(self, base: Path) -> dict:
        return _load_json(self._folder_exif_file(base), {})

    def _write_folder_exif(self, base: Path, data: dict):
        _save_json(self._folder_exif_file(base), data)

    # ---- get / set ----

    def get_hash(self, file_path: str, mtime: float, hash_type: str) -> Optional[dict]:
        resolved, base = self._resolve_path(file_path)
        key = _path_key(resolved, mtime)
        if self._mode == "folder" and base:
            hashes = self._read_folder_hashes(base)
        else:
            hashes = self._hashes
        bucket = hashes.get(key)
        if bucket and bucket.get("type") == f"hash_{hash_type}":
            return bucket.get("data")
        return None

    def set_hash(self, file_path: str, mtime: float, hash_type: str, signatures: dict):
        resolved, base = self._resolve_path(file_path)
        key = _path_key(resolved, mtime)
        entry = {
            "type": f"hash_{hash_type}",
            "file_path": resolved,
            "mtime": mtime,
            "data": signatures,
        }
        with self._lock:
            if self._mode == "folder" and base:
                hashes = self._read_folder_hashes(base)
                hashes[key] = entry
                self._write_folder_hashes(base, hashes)
            else:
                self._hashes[key] = entry
                self._flush_hashes()

    def get_embedding(self, file_path: str, mtime: float, model_name: str) -> Optional[np.ndarray]:
        resolved, base = self._resolve_path(file_path)
        key = _path_key(resolved, mtime)
        if self._mode == "folder" and base:
            emb_path = self._folder_emb_dir(base) / f"{key}_{model_name}.json"
        else:
            emb_path = PROJECT_CACHE_DIR / "embeddings" / f"{key}_{model_name}.json"
        if not emb_path.exists():
            return None
        data = _load_json(emb_path)
        if data and data.get("model") == model_name:
            return np.array(data["embedding"], dtype=np.float32)
        return None

    def set_embedding(self, file_path: str, mtime: float, model_name: str, embedding: np.ndarray):
        resolved, base = self._resolve_path(file_path)
        key = _path_key(resolved, mtime)
        emb_data = {
            "model": model_name,
            "file_path": resolved,
            "mtime": mtime,
            "embedding": embedding.astype(np.float32).tolist(),
        }
        with self._lock:
            if self._mode == "folder" and base:
                emb_path = self._folder_emb_dir(base) / f"{key}_{model_name}.json"
            else:
                emb_dir = PROJECT_CACHE_DIR / "embeddings"
                emb_dir.mkdir(parents=True, exist_ok=True)
                emb_path = emb_dir / f"{key}_{model_name}.json"
            _save_json(emb_path, emb_data)

    def get_exif(self, file_path: str, mtime: float) -> Optional[dict]:
        resolved, base = self._resolve_path(file_path)
        key = _path_key(resolved, mtime)
        if self._mode == "folder" and base:
            exif = self._read_folder_exif(base)
        else:
            exif = self._exif
        entry = exif.get(key)
        if entry:
            result = dict(entry.get("data", {}))
            if result.get("datetime") and isinstance(result["datetime"], str):
                result["datetime"] = datetime.fromisoformat(result["datetime"])
            return result
        return None

    def set_exif(self, file_path: str, mtime: float, feature: dict):
        resolved, base = self._resolve_path(file_path)
        key = _path_key(resolved, mtime)
        serializable = dict(feature)
        if serializable.get("datetime"):
            serializable["datetime"] = serializable["datetime"].isoformat()
        entry = {
            "file_path": resolved,
            "mtime": mtime,
            "data": serializable,
        }
        with self._lock:
            if self._mode == "folder" and base:
                exif = self._read_folder_exif(base)
                exif[key] = entry
                self._write_folder_exif(base, exif)
            else:
                self._exif[key] = entry
                self._flush_exif()

    # ---- stats / list ----

    def stats(self) -> dict:
        result: dict[str, int] = {}

        if self._mode == "folder":
            for d in self._scan_folder_caches():
                result["folder"] = result.get("folder", 0) + 1
            return result

        hash_types: dict[str, int] = {}
        for entry in self._hashes.values():
            t = entry.get("type", "unknown")
            hash_types[t] = hash_types.get(t, 0) + 1

        emb_types: dict[str, int] = {}
        emb_dir = PROJECT_CACHE_DIR / "embeddings"
        if emb_dir.exists():
            for f in emb_dir.glob("*.json"):
                model = f.stem.split("_", 1)[-1] if "_" in f.stem else "unknown"
                emb_types[f"emb_{model}"] = emb_types.get(f"emb_{model}", 0) + 1

        result.update(hash_types)
        result.update(emb_types)
        if self._exif:
            result["exif"] = len(self._exif)
        return result

    def list_entries(self, feature_type: Optional[str] = None) -> list[dict]:
        if self._mode == "folder":
            return self._list_folder_entries(feature_type)
        return self._list_project_entries(feature_type)

    def _list_project_entries(self, feature_type: Optional[str] = None) -> list[dict]:
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
            emb_dir = PROJECT_CACHE_DIR / "embeddings"
            if emb_dir.exists():
                for f in emb_dir.glob("*.json"):
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

    def _list_folder_entries(self, feature_type: Optional[str] = None) -> list[dict]:
        entries = []
        for cache_dir in self._scan_folder_cache_dirs():
            base = cache_dir.parent
            hashes = _load_json(cache_dir / "hashes.json", {})
            for key, entry in hashes.items():
                ft = entry.get("type", "")
                if feature_type and ft != feature_type:
                    continue
                abs_path = str(base / entry.get("file_path", "")) if entry.get("file_path") else ""
                entries.append({
                    "cache_key": key,
                    "feature_type": ft,
                    "file_path": abs_path,
                    "mtime": entry.get("mtime", 0),
                    "data": entry.get("data", {}),
                    "base_dir": str(base),
                })

            exif = _load_json(cache_dir / "exif.json", {})
            for key, entry in exif.items():
                if feature_type and "exif" != feature_type:
                    continue
                abs_path = str(base / entry.get("file_path", "")) if entry.get("file_path") else ""
                entries.append({
                    "cache_key": key,
                    "feature_type": "exif",
                    "file_path": abs_path,
                    "mtime": entry.get("mtime", 0),
                    "data": entry.get("data", {}),
                    "base_dir": str(base),
                })

            emb_dir = cache_dir / "embeddings"
            if emb_dir.exists():
                for f in emb_dir.glob("*.json"):
                    data = _load_json(f)
                    if not data:
                        continue
                    model = data.get("model", "unknown")
                    ft = f"emb_{model}"
                    if feature_type and ft != feature_type:
                        continue
                    abs_path = str(base / data.get("file_path", "")) if data.get("file_path") else ""
                    entries.append({
                        "cache_key": f.stem,
                        "feature_type": ft,
                        "file_path": abs_path,
                        "mtime": data.get("mtime", 0),
                        "data": {"model": model, "dim": len(data.get("embedding", []))},
                        "base_dir": str(base),
                    })

        return entries

    def _scan_folder_cache_dirs(self) -> list[Path]:
        dirs = []
        if (PROJECT_CACHE_DIR.parent.parent / "data" / "dirs.json").exists():
            dirs_data = _load_json(PROJECT_CACHE_DIR.parent.parent / "data" / "dirs.json", {})
            for v in dirs_data.values():
                p = Path(v.get("path", ""))
                cache_dir = _folder_cache_dir(p)
                if cache_dir.exists():
                    dirs.append(cache_dir)
        return dirs

    def _scan_folder_caches(self) -> list[Path]:
        result = []
        for cache_dir in self._scan_folder_cache_dirs():
            for f in cache_dir.iterdir():
                if f.name == "hashes.json":
                    data = _load_json(f, {})
                    result.extend(data.values())
                elif f.name == "exif.json":
                    data = _load_json(f, {})
                    result.extend(data.values())
                elif f.is_dir() and f.name == "embeddings":
                    result.extend(f.glob("*.json"))
        return result

    # ---- clear / delete ----

    def clear(self, feature_type: Optional[str] = None):
        with self._lock:
            if self._mode == "folder":
                for cache_dir in self._scan_folder_cache_dirs():
                    self._clear_folder_cache(cache_dir, feature_type)
                return

            if feature_type is None:
                self._hashes.clear()
                self._exif.clear()
                emb_dir = PROJECT_CACHE_DIR / "embeddings"
                if emb_dir.exists():
                    for f in emb_dir.glob("*.json"):
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
                emb_dir = PROJECT_CACHE_DIR / "embeddings"
                if emb_dir.exists():
                    for f in list(emb_dir.glob("*.json")):
                        if f.stem.endswith(f"_{model}"):
                            f.unlink()
            elif feature_type == "exif":
                self._exif.clear()
                self._flush_exif()

    def _clear_folder_cache(self, cache_dir: Path, feature_type: Optional[str] = None):
        if feature_type is None:
            import shutil
            if cache_dir.exists():
                shutil.rmtree(cache_dir)
        elif feature_type.startswith("hash_"):
            f = cache_dir / "hashes.json"
            if f.exists():
                data = _load_json(f, {})
                data = {k: v for k, v in data.items() if v.get("type") != feature_type}
                _save_json(f, data)
        elif feature_type == "exif":
            f = cache_dir / "exif.json"
            if f.exists():
                _save_json(f, {})
        elif feature_type.startswith("emb_"):
            model = feature_type[4:]
            emb_dir = cache_dir / "embeddings"
            if emb_dir.exists():
                for f in list(emb_dir.glob("*.json")):
                    if f.stem.endswith(f"_{model}"):
                        f.unlink()

    def delete_entry(self, cache_key: str) -> bool:
        with self._lock:
            if self._mode == "folder":
                for cache_dir in self._scan_folder_cache_dirs():
                    if self._delete_from_folder(cache_dir, cache_key):
                        return True
                return False

            if cache_key in self._hashes:
                del self._hashes[cache_key]
                self._flush_hashes()
                return True
            if cache_key in self._exif:
                del self._exif[cache_key]
                self._flush_exif()
                return True
            emb_file = PROJECT_CACHE_DIR / "embeddings" / f"{cache_key}.json"
            if emb_file.exists():
                emb_file.unlink()
                return True
        return False

    def _delete_from_folder(self, cache_dir: Path, cache_key: str) -> bool:
        hashes_file = cache_dir / "hashes.json"
        if hashes_file.exists():
            data = _load_json(hashes_file, {})
            if cache_key in data:
                del data[cache_key]
                _save_json(hashes_file, data)
                return True

        exif_file = cache_dir / "exif.json"
        if exif_file.exists():
            data = _load_json(exif_file, {})
            if cache_key in data:
                del data[cache_key]
                _save_json(exif_file, data)
                return True

        emb_dir = cache_dir / "embeddings"
        if emb_dir.exists():
            emb_file = emb_dir / f"{cache_key}.json"
            if emb_file.exists():
                emb_file.unlink()
                return True
        return False

    # ---- conversion ----

    def export_to_folder(self, dir_paths: Optional[list[str]] = None) -> dict:
        migrated = 0
        entries = self._list_project_entries()

        groups: dict[str, list] = {}
        for entry in entries:
            fp = entry.get("file_path", "")
            if not fp:
                continue

            matched_dir = None
            if dir_paths:
                for dp in dir_paths:
                    if fp.startswith(dp + "/") or fp.startswith(dp + os.sep):
                        matched_dir = dp
                        break
                if not matched_dir:
                    p = Path(fp)
                    if p.exists():
                        for dp in dir_paths:
                            try:
                                p.resolve().relative_to(Path(dp).resolve())
                                matched_dir = dp
                                break
                            except ValueError:
                                continue
                if not matched_dir:
                    continue

            if matched_dir:
                base = Path(matched_dir)
                p = Path(fp)
            else:
                p = Path(fp)
                if not p.exists():
                    continue
                base = _find_base_dir(str(p))

            base_str = str(base)
            if base_str not in groups:
                groups[base_str] = []
            groups[base_str].append((entry, p))

        for base_str, items in groups.items():
            base = Path(base_str)
            cache_dir = _folder_cache_dir(base)
            cache_dir.mkdir(parents=True, exist_ok=True)

            hashes = _load_json(cache_dir / "hashes.json", {})
            exif_data = _load_json(cache_dir / "exif.json", {})

            for entry, p in items:
                try:
                    rel = str(p.resolve().relative_to(base))
                except ValueError:
                    rel = str(p)

                new_key = _path_key(rel, entry["mtime"])
                ft = entry["feature_type"]

                if ft.startswith("hash_"):
                    hashes[new_key] = {
                        "type": ft,
                        "file_path": rel,
                        "mtime": entry["mtime"],
                        "data": entry["data"],
                    }
                elif ft == "exif":
                    exif_data[new_key] = {
                        "file_path": rel,
                        "mtime": entry["mtime"],
                        "data": entry["data"],
                    }
                elif ft.startswith("emb_"):
                    old_key = entry["cache_key"]
                    old_file = PROJECT_CACHE_DIR / "embeddings" / f"{old_key}.json"
                    if old_file.exists():
                        emb_data = _load_json(old_file)
                        if emb_data:
                            emb_data["file_path"] = rel
                            emb_dir = cache_dir / "embeddings"
                            emb_dir.mkdir(parents=True, exist_ok=True)
                            _save_json(emb_dir / f"{new_key}_{emb_data.get('model', 'unknown')}.json", emb_data)

                migrated += 1

            _save_json(cache_dir / "hashes.json", hashes)
            _save_json(cache_dir / "exif.json", exif_data)

        return {"migrated": migrated, "directories": len(groups)}

    def import_from_folder(self, dir_paths: Optional[list[str]] = None) -> dict:
        migrated = 0
        scan_dirs = [Path(p) for p in dir_paths] if dir_paths else [Path(v.get("path", "")) for v in _load_json(PROJECT_CACHE_DIR.parent.parent / "data" / "dirs.json", {}).values()]
        scan_dirs = [d for d in scan_dirs if d.exists()]

        for base in scan_dirs:
            cache_dir = _folder_cache_dir(base)
            if not cache_dir.exists():
                continue

            hashes = _load_json(cache_dir / "hashes.json", {})
            for key, entry in hashes.items():
                rel = entry.get("file_path", "")
                abs_path = str(base / rel)
                new_key = _path_key(abs_path, entry.get("mtime", 0))
                self._hashes[new_key] = {
                    "type": entry.get("type", ""),
                    "file_path": abs_path,
                    "mtime": entry.get("mtime", 0),
                    "data": entry.get("data", {}),
                }
                migrated += 1

            exif_data = _load_json(cache_dir / "exif.json", {})
            for key, entry in exif_data.items():
                rel = entry.get("file_path", "")
                abs_path = str(base / rel)
                new_key = _path_key(abs_path, entry.get("mtime", 0))
                self._exif[new_key] = {
                    "file_path": abs_path,
                    "mtime": entry.get("mtime", 0),
                    "data": entry.get("data", {}),
                }
                migrated += 1

            emb_dir = cache_dir / "embeddings"
            if emb_dir.exists():
                for f in emb_dir.glob("*.json"):
                    data = _load_json(f)
                    if not data:
                        continue
                    rel = data.get("file_path", "")
                    abs_path = str(base / rel)
                    new_key = _path_key(abs_path, data.get("mtime", 0))
                    model = data.get("model", "unknown")
                    out_dir = PROJECT_CACHE_DIR / "embeddings"
                    out_dir.mkdir(parents=True, exist_ok=True)
                    data["file_path"] = abs_path
                    _save_json(out_dir / f"{new_key}_{model}.json", data)
                    migrated += 1

        self._flush_hashes()
        self._flush_exif()
        return {"migrated": migrated, "directories": len(scan_dirs)}


cache = FeatureCache()
