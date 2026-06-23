from abc import ABC, abstractmethod
from pathlib import Path
from typing import Optional
from .base import ImageItem, ImageGroup


class Deduplicator(ABC):
    @abstractmethod
    def compute_signature(self, item: ImageItem) -> any:
        raise NotImplementedError

    @abstractmethod
    def are_similar(self, sig1: any, sig2: any) -> bool:
        raise NotImplementedError

    @abstractmethod
    def group_items(self, items: list[ImageItem]) -> list[ImageGroup]:
        raise NotImplementedError

    def find_groups(
        self,
        image_paths: list[Path | str],
        threshold: float = 0.9,
    ) -> list[ImageGroup]:
        items = []
        for p in image_paths:
            item = ImageItem(path=Path(p))
            items.append(item)

        for item in items:
            item.metadata["signature"] = self.compute_signature(item)

        return self.group_items(items)

    def deduplicate(
        self,
        image_paths: list[Path | str],
        threshold: float = 0.9,
    ) -> tuple[list[ImageGroup], list[ImageItem]]:
        groups = self.find_groups(image_paths, threshold)
        all_duplicates = []
        for group in groups:
            all_duplicates.extend(group.get_duplicates())
        return groups, all_duplicates


class Grouper(ABC):
    @abstractmethod
    def extract_feature(self, item: ImageItem) -> any:
        raise NotImplementedError

    @abstractmethod
    def should_group(self, feat1: any, feat2: any) -> bool:
        raise NotImplementedError

    def cluster(
        self,
        image_paths: list[Path | str],
        **kwargs
    ) -> list[ImageGroup]:
        items = []
        for p in image_paths:
            item = ImageItem(path=Path(p))
            items.append(item)

        for item in items:
            item.metadata["feature"] = self.extract_feature(item)

        return self._cluster_items(items, **kwargs)

    @abstractmethod
    def _cluster_items(self, items: list[ImageItem], **kwargs) -> list[ImageGroup]:
        raise NotImplementedError
