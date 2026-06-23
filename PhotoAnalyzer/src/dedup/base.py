from dataclasses import dataclass, field
from pathlib import Path
from typing import Optional


@dataclass
class ImageItem:
    path: Path
    hash_value: Optional[str] = None
    exif_datetime: Optional[str] = None
    embedding: Optional[list[float]] = None
    file_size: int = 0
    width: Optional[int] = None
    height: Optional[int] = None
    metadata: dict = field(default_factory=dict)


@dataclass
class ImageGroup:
    group_id: str
    items: list[ImageItem] = field(default_factory=list)
    representative: Optional[ImageItem] = None

    def add(self, item: ImageItem):
        self.items.append(item)
        if self.representative is None:
            self.representative = item

    def __len__(self) -> int:
        return len(self.items)

    def get_duplicates(self) -> list[ImageItem]:
        return self.items[1:] if len(self.items) > 1 else []
