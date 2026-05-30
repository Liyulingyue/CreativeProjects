from abc import ABC, abstractmethod
from typing import Optional
from dataclasses import dataclass

from src.task_type import TaskType


@dataclass
class LabelItem:
    id: str
    text: str
    label: Optional[str] = None
    confidence: Optional[float] = None
    source: str = "llm"  # "llm" | "small_model"



class BaseLabeler(ABC):
    def __init__(self, categories: list[str]):
        self.categories = categories

    @property
    @abstractmethod
    def task_type(self) -> TaskType:
        pass

    @abstractmethod
    def label(self, item, instruction: str = "") -> LabelItem:
        pass

    @abstractmethod
    def label_batch(self, items: list, instruction: str = "", show_progress: bool = True) -> list[LabelItem]:
        pass

    def save(self, items: list[LabelItem], output_path: str):
        import json
        from pathlib import Path
        Path(output_path).parent.mkdir(parents=True, exist_ok=True)
        with open(output_path, "w", encoding="utf-8") as f:
            for item in items:
                f.write(json.dumps({
                    "id": item.id,
                    "text": item.text,
                    "label": item.label,
                    "confidence": item.confidence,
                    "source": item.source
                }, ensure_ascii=False) + "\n")


class BaseClassifier(ABC):
    @abstractmethod
    def load(self):
        pass

    @abstractmethod
    def classify(self, item, instruction: str = "") -> LabelItem:
        pass

    @abstractmethod
    def classify_batch(self, items: list, instruction: str = "", show_progress: bool = True) -> list[LabelItem]:
        pass

    def save(self, items: list[LabelItem], output_path: str):
        import json
        from pathlib import Path
        Path(output_path).parent.mkdir(parents=True, exist_ok=True)
        with open(output_path, "w", encoding="utf-8") as f:
            for item in items:
                f.write(json.dumps({
                    "id": item.id,
                    "text": item.text,
                    "label": item.label,
                    "confidence": item.confidence,
                    "source": item.source
                }, ensure_ascii=False) + "\n")


class BaseTrainer(ABC):
    def __init__(self, config):
        self.config = config

    @property
    @abstractmethod
    def task_type(self) -> TaskType:
        pass

    @abstractmethod
    def train(self, labeled_path: str):
        pass
