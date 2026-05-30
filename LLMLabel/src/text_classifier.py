import json
import torch
import torch.nn.functional as F
from pathlib import Path
from dataclasses import dataclass
from typing import Optional

from transformers import AutoTokenizer, AutoModelForSequenceClassification

from src.base import BaseClassifier, LabelItem
from src.task_type import TaskType


@dataclass
class TextClassifyConfig:
    model_path: str = "LLMLabel/data/output/model/final"
    max_seq_length: int = 512
    device: str = "cuda"


class TextClassifier(BaseClassifier):
    def __init__(self, config: Optional[TextClassifyConfig] = None):
        self.config = config or TextClassifyConfig()
        self.model = None
        self.tokenizer = None
        self.label2id: dict = {}
        self.id2label: dict = {}
        self._loaded = False

    @property
    def task_type(self) -> TaskType:
        return TaskType.TEXT_CLASSIFICATION

    def load(self):
        if self._loaded:
            return

        label_map_path = Path(self.config.model_path) / "label_map.json"
        if label_map_path.exists():
            with open(label_map_path, "r", encoding="utf-8") as f:
                data = json.load(f)
                self.label2id = data["label2id"]
                self.id2label = {int(k): v for k, v in data["id2label"].items()}
        else:
            raise FileNotFoundError(f"label_map.json not found in {self.config.model_path}")

        print(f"[TextClassifier] 加载模型：{self.config.model_path}")
        self.tokenizer = AutoTokenizer.from_pretrained(self.config.model_path)
        self.model = AutoModelForSequenceClassification.from_pretrained(self.config.model_path)

        device = torch.device(self.config.device if torch.cuda.is_available() else "cpu")
        self.model.to(device)
        self.model.eval()

        self._loaded = True
        print(f"[TextClassifier] 模型加载完成，类别：{self.id2label}")

    def classify(self, text: str, instruction: str = "") -> LabelItem:
        self.load()

        prompt = f"{instruction}\n\n{text}" if instruction else text
        inputs = self.tokenizer(
            prompt,
            return_tensors="pt",
            truncation=True,
            max_length=self.config.max_seq_length,
            padding=True,
        )

        device = next(self.model.parameters()).device
        inputs = {k: v.to(device) for k, v in inputs.items()}

        with torch.no_grad():
            outputs = self.model(**inputs)
            probs = F.softmax(outputs.logits, dim=-1)
            confidence, pred_id = torch.max(probs, dim=-1)

        pred_id = pred_id.item()
        confidence = round(probs[0][pred_id].item(), 4)
        label = self.id2label.get(pred_id, "unknown")

        return LabelItem(
            id="",
            text=text,
            label=label,
            confidence=confidence,
            source="small_model"
        )

    def classify_batch(
        self,
        texts: list[str],
        instruction: str = "",
        show_progress: bool = True
    ) -> list[LabelItem]:
        results = []
        for i, text in enumerate(texts):
            item = self.classify(text, instruction)
            item.id = f"sm_{i}"
            results.append(item)
            if show_progress and (i + 1) % 500 == 0:
                print(f"[TextClassifier] 进度：{i + 1}/{len(texts)}")
        return results


Classifier = TextClassifier
