from enum import Enum


class TaskType(Enum):
    TEXT_CLASSIFICATION = "text_classification"
    IMAGE_CLASSIFICATION = "image_classification"

    @staticmethod
    def from_string(s: str) -> "TaskType":
        mapping = {
            "text": TaskType.TEXT_CLASSIFICATION,
            "text_classification": TaskType.TEXT_CLASSIFICATION,
            "image": TaskType.IMAGE_CLASSIFICATION,
            "image_classification": TaskType.IMAGE_CLASSIFICATION,
        }
        key = s.lower().strip()
        if key not in mapping:
            raise ValueError(f"Unknown task type: {s}, available: {list(mapping.keys())}")
        return mapping[key]
