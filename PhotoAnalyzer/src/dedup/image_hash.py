import os
import imagehash
from PIL import Image
from pathlib import Path
from typing import Optional
from .base import ImageItem, ImageGroup
from .deduplicator import Deduplicator
from .cache import cache


class PHashDeduplicator(Deduplicator):
    def __init__(
        self,
        hash_size: int = 8,
        threshold: float = 5,
    ):
        self.hash_size = hash_size
        self.threshold = threshold

    def compute_signature(self, item: ImageItem) -> str:
        try:
            mtime = os.path.getmtime(item.path)
            cached = cache.get_hash(str(item.path), mtime, "phash")
            if cached and "phash" in cached:
                item.hash_value = cached["phash"]
                return item.hash_value

            img = Image.open(item.path)
            item.hash_value = str(imagehash.phash(img, hash_size=self.hash_size))
            cache.set_hash(str(item.path), mtime, "phash", {"phash": item.hash_value})
            return item.hash_value
        except Exception:
            return ""

    def are_similar(self, sig1: str, sig2: str) -> bool:
        if not sig1 or not sig2:
            return False
        h1 = imagehash.hex_to_hash(sig1)
        h2 = imagehash.hex_to_hash(sig2)
        distance = h1 - h2
        return distance <= self.threshold

    def group_items(self, items: list[ImageItem]) -> list[ImageGroup]:
        groups: dict[str, ImageGroup] = {}
        visited: set[int] = set()

        for i, item in enumerate(items):
            if i in visited:
                continue

            sig = item.metadata.get("signature", "")
            if not sig:
                continue

            group = ImageGroup(group_id=f"phash_group_{len(groups)}")
            group.add(item)
            visited.add(i)

            for j, other in enumerate(items):
                if j in visited or j == i:
                    continue

                other_sig = other.metadata.get("signature", "")
                if self.are_similar(sig, other_sig):
                    group.add(other)
                    visited.add(j)

            groups[group.group_id] = group

        return list(groups.values())


class AverageHashDeduplicator(Deduplicator):
    def __init__(self, hash_size: int = 8, threshold: float = 5):
        self.hash_size = hash_size
        self.threshold = threshold

    def compute_signature(self, item: ImageItem) -> str:
        try:
            mtime = os.path.getmtime(item.path)
            cached = cache.get_hash(str(item.path), mtime, "ahash")
            if cached and "ahash" in cached:
                item.hash_value = cached["ahash"]
                return item.hash_value

            img = Image.open(item.path)
            item.hash_value = str(imagehash.average_hash(img, hash_size=self.hash_size))
            cache.set_hash(str(item.path), mtime, "ahash", {"ahash": item.hash_value})
            return item.hash_value
        except Exception:
            return ""

    def are_similar(self, sig1: str, sig2: str) -> bool:
        if not sig1 or not sig2:
            return False
        h1 = imagehash.hex_to_hash(sig1)
        h2 = imagehash.hex_to_hash(sig2)
        return (h1 - h2) <= self.threshold

    def group_items(self, items: list[ImageItem]) -> list[ImageGroup]:
        return PHashDeduplicator(self.hash_size, self.threshold).group_items(items)


class DHashDeduplicator(Deduplicator):
    def __init__(self, hash_size: int = 8, threshold: float = 5):
        self.hash_size = hash_size
        self.threshold = threshold

    def compute_signature(self, item: ImageItem) -> str:
        try:
            mtime = os.path.getmtime(item.path)
            cached = cache.get_hash(str(item.path), mtime, "dhash")
            if cached and "dhash" in cached:
                item.hash_value = cached["dhash"]
                return item.hash_value

            img = Image.open(item.path)
            item.hash_value = str(imagehash.dhash(img, hash_size=self.hash_size))
            cache.set_hash(str(item.path), mtime, "dhash", {"dhash": item.hash_value})
            return item.hash_value
        except Exception:
            return ""

    def are_similar(self, sig1: str, sig2: str) -> bool:
        if not sig1 or not sig2:
            return False
        h1 = imagehash.hex_to_hash(sig1)
        h2 = imagehash.hex_to_hash(sig2)
        return (h1 - h2) <= self.threshold

    def group_items(self, items: list[ImageItem]) -> list[ImageGroup]:
        return PHashDeduplicator(self.hash_size, self.threshold).group_items(items)


class MultiHashDeduplicator(Deduplicator):
    def __init__(
        self,
        hash_size: int = 8,
        threshold: float = 5,
        hash_funcs: Optional[list] = None,
    ):
        self.hash_size = hash_size
        self.threshold = threshold
        self.hash_funcs = hash_funcs or ["phash", "ahash", "dhash"]

    def compute_signature(self, item: ImageItem) -> dict[str, str]:
        try:
            mtime = os.path.getmtime(item.path)
            cached = cache.get_hash(str(item.path), mtime, "multihash")
            if cached:
                missing = [h for h in self.hash_funcs if h not in cached]
                if not missing:
                    item.hash_value = cached
                    return signatures if (signatures := {k: v for k, v in cached.items() if k in self.hash_funcs}) else cached

            img = Image.open(item.path)
            signatures = {}
            if "phash" in self.hash_funcs:
                signatures["phash"] = str(imagehash.phash(img, hash_size=self.hash_size))
            if "ahash" in self.hash_funcs:
                signatures["ahash"] = str(imagehash.average_hash(img, hash_size=self.hash_size))
            if "dhash" in self.hash_funcs:
                signatures["dhash"] = str(imagehash.dhash(img, hash_size=self.hash_size))
            if "whash" in self.hash_funcs:
                signatures["whash"] = str(imagehash.whash(img, hash_size=self.hash_size))
            item.hash_value = signatures
            cache.set_hash(str(item.path), mtime, "multihash", signatures)
            return signatures
        except Exception:
            return {}

    def are_similar(self, sig1: dict, sig2: dict) -> bool:
        if not sig1 or not sig2:
            return False
        match_count = 0
        total_count = len(sig1)
        for key in sig1:
            if key in sig2:
                h1 = imagehash.hex_to_hash(sig1[key])
                h2 = imagehash.hex_to_hash(sig2[key])
                if (h1 - h2) <= self.threshold:
                    match_count += 1
        return match_count / total_count >= 0.5

    def group_items(self, items: list[ImageItem]) -> list[ImageGroup]:
        groups: dict[str, ImageGroup] = {}
        visited: set[int] = set()

        for i, item in enumerate(items):
            if i in visited:
                continue

            sig = item.metadata.get("signature", {})
            if not sig:
                continue

            group = ImageGroup(group_id=f"multihash_group_{len(groups)}")
            group.add(item)
            visited.add(i)

            for j, other in enumerate(items):
                if j in visited or j == i:
                    continue

                other_sig = other.metadata.get("signature", {})
                if self.are_similar(sig, other_sig):
                    group.add(other)
                    visited.add(j)

            groups[group.group_id] = group

        return list(groups.values())
