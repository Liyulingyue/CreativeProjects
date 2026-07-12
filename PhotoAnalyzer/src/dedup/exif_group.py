import os
from datetime import datetime, timedelta
from pathlib import Path
from typing import Optional
from PIL import Image
from PIL.ExifTags import TAGS
from .base import ImageItem, ImageGroup
from .deduplicator import Grouper
from .cache import cache


class ExifGrouper(Grouper):
    def __init__(
        self,
        time_window_seconds: int = 5,
        group_by_date: bool = False,
    ):
        self.time_window = timedelta(seconds=time_window_seconds)
        self.group_by_date = group_by_date

    def extract_feature(self, item: ImageItem) -> Optional[datetime]:
        try:
            mtime = os.path.getmtime(item.path)
            cached = cache.get_exif(str(item.path), mtime)
            if cached is not None:
                if cached.get("datetime"):
                    item.exif_datetime = cached["datetime"].strftime("%Y:%m:%d %H:%M:%S") if isinstance(cached["datetime"], datetime) else str(cached["datetime"])
                return cached.get("datetime")

            img = Image.open(item.path)
            exif = img._getexif()
            if not exif:
                return None

            for tag_id, value in exif.items():
                tag = TAGS.get(tag_id, tag_id)
                if tag == "DateTimeOriginal":
                    item.exif_datetime = value
                    dt = datetime.strptime(value, "%Y:%m:%d %H:%M:%S")
                    cache.set_exif(str(item.path), mtime, {"datetime": dt})
                    return dt
                elif tag == "DateTime":
                    item.exif_datetime = value
                    dt = datetime.strptime(value, "%Y:%m:%d %H:%M:%S")
                    cache.set_exif(str(item.path), mtime, {"datetime": dt})
                    return dt
            return None
        except Exception:
            return None

    def should_group(self, feat1, feat2) -> bool:
        if feat1 is None or feat2 is None:
            return False
        if self.group_by_date:
            return feat1.date() == feat2.date()
        return abs(feat1 - feat2) <= self.time_window

    def _cluster_items(self, items: list[ImageItem], **kwargs) -> list[ImageGroup]:
        groups: dict[str, ImageGroup] = {}
        visited: set[int] = set()

        for i, item in enumerate(items):
            if i in visited:
                continue

            feat = item.metadata.get("feature")
            if feat is None:
                continue

            if self.group_by_date:
                group_id = feat.date().isoformat()
            else:
                group_id = f"burst_{feat.strftime('%Y%m%d_%H%M%S')}"

            if group_id not in groups:
                groups[group_id] = ImageGroup(group_id=group_id)

            groups[group_id].add(item)
            visited.add(i)

            for j, other in enumerate(items):
                if j in visited or j == i:
                    continue

                other_feat = other.metadata.get("feature")
                if self.should_group(feat, other_feat):
                    if self.group_by_date:
                        groups[group_id].add(other)
                    else:
                        if other_feat and abs(other_feat - feat) <= self.time_window:
                            groups[group_id].add(other)
                    visited.add(j)

        return list(groups.values())


class SequentialGrouper(Grouper):
    def __init__(
        self,
        time_window_seconds: int = 5,
        same_camera_only: bool = True,
    ):
        self.time_window = timedelta(seconds=time_window_seconds)
        self.same_camera_only = same_camera_only

    def extract_feature(self, item: ImageItem) -> Optional[dict]:
        try:
            mtime = os.path.getmtime(item.path)
            cached = cache.get_exif(str(item.path), mtime)
            if cached is not None:
                if cached.get("datetime") and isinstance(cached["datetime"], datetime):
                    item.exif_datetime = cached["datetime"].strftime("%Y:%m:%d %H:%M:%S")
                return cached

            img = Image.open(item.path)
            exif = img._getexif()
            if not exif:
                return None

            feature = {"datetime": None, "camera_make": None, "camera_model": None}

            for tag_id, value in exif.items():
                tag = TAGS.get(tag_id, tag_id)
                if tag == "DateTimeOriginal":
                    feature["datetime"] = datetime.strptime(value, "%Y:%m:%d %H:%M:%S")
                    item.exif_datetime = value
                elif tag == "Make":
                    feature["camera_make"] = str(value).strip()
                elif tag == "Model":
                    feature["camera_model"] = str(value).strip()

            if feature["datetime"] is not None:
                cache.set_exif(str(item.path), mtime, feature)
            return feature
        except Exception:
            return None

    def should_group(self, feat1: dict, feat2: dict) -> bool:
        if not feat1 or not feat2:
            return False
        if feat1.get("datetime") is None or feat2.get("datetime") is None:
            return False
        if self.same_camera_only:
            if feat1.get("camera_make") != feat2.get("camera_make"):
                return False
            if feat1.get("camera_model") != feat2.get("camera_model"):
                return False
        return abs(feat1["datetime"] - feat2["datetime"]) <= self.time_window

    def _cluster_items(self, items: list[ImageItem], **kwargs) -> list[ImageGroup]:
        groups: list[ImageGroup] = []
        visited: set[int] = set()

        for i, item in enumerate(items):
            if i in visited:
                continue

            feat = item.metadata.get("feature")
            if not feat or feat.get("datetime") is None:
                continue

            group = ImageGroup(group_id=f"sequential_group_{len(groups)}")
            group.add(item)
            visited.add(i)

            for j, other in enumerate(items):
                if j in visited or j == i:
                    continue

                other_feat = other.metadata.get("feature")
                if self.should_group(feat, other_feat):
                    group.add(other)
                    visited.add(j)

            groups.append(group)

        return groups
