import json
import torch
from pathlib import Path
from dataclasses import dataclass, field
from PIL import Image
import pandas as pd

from transformers import AutoModelForImageClassification, AutoImageProcessor, TrainingArguments, Trainer
from datasets import Dataset, Image as HfImage
from torchvision.transforms import Compose, Resize, CenterCrop, ToTensor, Normalize

from src.base import BaseTrainer
from src.task_type import TaskType


@dataclass
class ImageTrainConfig:
    model_name: str = "google/vit-base-patch16-224"
    output_dir: str = "LLMLabel/data/output/image_model"
    num_epochs: int = 3
    batch_size: int = 16
    learning_rate: float = 2e-5
    save_steps: int = 100
    eval_steps: int = 100
    warmup_steps: int = 100
    label2id: dict = field(default_factory=dict)
    id2label: dict = field(default_factory=dict)


class ImageTrainer(BaseTrainer):
    def __init__(self, config=None):
        super().__init__(config or ImageTrainConfig())
        self.label2id: dict[str, int] = {}
        self.id2label: dict[int, str] = {}

    @property
    def task_type(self) -> TaskType:
        return TaskType.IMAGE_CLASSIFICATION

    def load_labeled_data(self, labeled_path: str):
        records = []
        with open(labeled_path, "r", encoding="utf-8") as f:
            for line in f:
                obj = json.loads(line.strip())
                if obj.get("label") is not None:
                    records.append({
                        "image_path": obj["image_path"],
                        "label": obj["label"],
                    })
        return records

    def build_label_maps(self, records: list):
        unique_labels = sorted(set(r["label"] for r in records))
        self.label2id = {label: i for i, label in enumerate(unique_labels)}
        self.id2label = {i: label for i, label in enumerate(unique_labels)}
        self.config.label2id = self.label2id
        self.config.id2label = self.id2label

    def train(self, labeled_path: str):
        print(f"[ImageTrainer] 加载标注数据：{labeled_path}")
        records = self.load_labeled_data(labeled_path)
        print(f"[ImageTrainer] 共 {len(records)} 条标注记录")

        self.build_label_maps(records)
        num_labels = len(self.label2id)
        print(f"[ImageTrainer] 类别映射：{self.label2id}")
        print(f"[ImageTrainer] 加载模型：{self.config.model_name}")

        processor = AutoImageProcessor.from_pretrained(self.config.model_name)
        model = AutoModelForImageClassification.from_pretrained(
            self.config.model_name,
            num_labels=num_labels,
        )

        df = pd.DataFrame(records)
        df["label_id"] = df["label"].map(self.label2id)
        dataset = Dataset.from_pandas(df).cast_column("image_path", HfImage())

        def preprocess(batch):
            inputs = processor(
                [Image.open(Path(p).as_posix()).convert("RGB") for p in batch["image_path"]],
                return_tensors="pt",
            )
            return {
                "pixel_values": inputs["pixel_values"],
                "label": batch["label_id"]
            }

        dataset = dataset.map(preprocess, batched=True, remove_columns=["image_path", "label"])
        dataset.set_format("torch", columns=["pixel_values", "label"])

        output_dir = Path(self.config.output_dir)
        output_dir.mkdir(parents=True, exist_ok=True)

        training_args = TrainingArguments(
            output_dir=str(output_dir),
            num_train_epochs=self.config.num_epochs,
            per_device_train_batch_size=self.config.batch_size,
            learning_rate=self.config.learning_rate,
            warmup_steps=self.config.warmup_steps,
            save_steps=self.config.save_steps,
            logging_steps=self.config.eval_steps,
            bf16=torch.cuda.is_available(),
            report_to="none",
        )

        hf_trainer = Trainer(
            model=model,
            args=training_args,
            train_dataset=dataset,
        )

        print("[ImageTrainer] 开始训练...")
        hf_trainer.train()

        out_path = output_dir / "final"
        model.save_pretrained(str(out_path))
        processor.save_pretrained(str(out_path))

        with open(out_path / "label_map.json", "w", encoding="utf-8") as f:
            json.dump({
                "label2id": self.label2id,
                "id2label": self.id2label,
            }, f, ensure_ascii=False, indent=2)

        print(f"[ImageTrainer] 训练完成，模型保存至：{out_path}")
