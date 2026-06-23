from pathlib import Path
from typing import Optional, Union
from .base import ImageItem, ImageGroup
from .image_hash import PHashDeduplicator, MultiHashDeduplicator
from .exif_group import ExifGrouper, SequentialGrouper
from .embedding import create_embedding_grouper


class CompositeDeduplicator:
    def __init__(
        self,
        use_exif: bool = True,
        use_phash: bool = True,
        use_embedding: bool = False,
        embedding_model: str = "clip",
        exif_time_window: int = 5,
        phash_threshold: int = 5,
        embedding_threshold: float = 0.85,
    ):
        self.use_exif = use_exif
        self.use_phash = use_phash
        self.use_embedding = use_embedding

        self.exif_grouper = SequentialGrouper(time_window_seconds=exif_time_window) if use_exif else None
        self.phash_dedup = MultiHashDeduplicator(threshold=phash_threshold) if use_phash else None
        self.embedding_grouper = create_embedding_grouper(embedding_model, similarity_threshold=embedding_threshold) if use_embedding else None

    def group_all(
        self,
        image_paths: list[Path | str],
    ) -> list[ImageGroup]:
        all_groups = []
        seen_paths: set[str] = set()

        if self.use_exif and self.exif_grouper:
            exif_groups = self.exif_grouper.cluster(list(image_paths))
            for group in exif_groups:
                if len(group) > 1:
                    for item in group.items:
                        seen_paths.add(str(item.path))
                    all_groups.append(group)

        remaining_paths = [p for p in image_paths if str(p) not in seen_paths]

        if self.use_phash and self.phash_dedup and remaining_paths:
            phash_groups = self.phash_dedup.find_groups(remaining_paths)
            for group in phash_groups:
                if len(group) > 1:
                    for item in group.items:
                        seen_paths.add(str(item.path))
                    all_groups.append(group)

        remaining_paths = [p for p in image_paths if str(p) not in seen_paths]

        if self.use_embedding and self.embedding_grouper and remaining_paths:
            emb_groups = self.embedding_grouper.cluster(remaining_paths)
            for group in emb_groups:
                if len(group) > 1:
                    all_groups.append(group)

        return all_groups

    def find_duplicates(
        self,
        image_paths: list[Path | str],
    ) -> tuple[list[ImageGroup], list[ImageItem]]:
        groups = self.group_all(image_paths)
        all_duplicates = []
        for group in groups:
            all_duplicates.extend(group.get_duplicates())
        return groups, all_duplicates

    def get_best_from_each_group(
        self,
        groups: list[ImageGroup],
    ) -> list[ImageItem]:
        best_items = []
        for group in groups:
            if len(group) == 0:
                continue
            best = group.representative or group.items[0]
            best_items.append(best)
        return best_items


class HierarchicalDeduplicator:
    def __init__(
        self,
        stages: Optional[list] = None,
    ):
        if stages is None:
            stages = [
                {"type": "exif", "time_window": 5},
                {"type": "phash", "threshold": 5},
                {"type": "embedding", "model": "clip", "threshold": 0.85},
            ]
        self.stages = stages

    def _create_stage(self, config: dict):
        stage_type = config.get("type")
        if stage_type == "exif":
            return SequentialGrouper(time_window_seconds=config.get("time_window", 5))
        elif stage_type == "phash":
            return MultiHashDeduplicator(threshold=config.get("threshold", 5))
        elif stage_type == "embedding":
            return create_embedding_grouper(
                model=config.get("model", "clip"),
                similarity_threshold=config.get("threshold", 0.85),
            )
        else:
            raise ValueError(f"未知的去重阶段类型: {stage_type}")

    def deduplicate(
        self,
        image_paths: list[Path | str],
    ) -> dict:
        remaining = list(image_paths)
        all_groups = []
        stage_names = []

        for i, stage_config in enumerate(self.stages):
            if not remaining:
                break

            stage = self._create_stage(stage_config)
            stage_name = f"stage{i+1}_{stage_config.get('type')}"
            stage_names.append(stage_name)

            if hasattr(stage, "find_groups"):
                groups = stage.find_groups(remaining)
            else:
                groups = stage.cluster(remaining)

            groups_with_dups = [g for g in groups if len(g) > 1]

            for group in groups_with_dups:
                all_groups.append({
                    "stage": stage_name,
                    "group": group,
                    "duplicate_count": len(group) - 1,
                })

            processed_paths = set()
            for group in groups:
                for item in group.items:
                    processed_paths.add(str(item.path))

            remaining = [p for p in remaining if str(p) not in processed_paths]

        return {
            "groups": all_groups,
            "remaining_unique": remaining,
            "total_duplicates": sum(g["duplicate_count"] for g in all_groups),
            "stages_applied": stage_names,
        }
