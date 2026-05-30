import base64
from typing import Optional
from openai import OpenAI

from src.base import BaseLabeler, LabelItem
from src.task_type import TaskType


class ImageLLMLabeler(BaseLabeler):
    def __init__(
        self,
        api_key: str,
        base_url: str = "https://api.openai.com/v1",
        model_name: str = "gpt-4o",
        categories: list[str] | None = None,
    ):
        super().__init__(categories or ["正例", "负例"])
        self.client = OpenAI(api_key=api_key, base_url=base_url)
        self.model_name = model_name

    @property
    def task_type(self) -> TaskType:
        return TaskType.IMAGE_CLASSIFICATION

    def _encode_image(self, image_path: str) -> str:
        with open(image_path, "rb") as f:
            return base64.b64encode(f.read()).decode("utf-8")

    def label(self, image_path: str, instruction: str = "") -> LabelItem:
        b64_image = self._encode_image(image_path)
        prompt = f"{instruction}\n\n分类选项：{'、'.join(self.categories)}" if instruction else f"分类选项：{'、'.join(self.categories)}"

        response = self.client.chat.completions.create(
            model=self.model_name,
            messages=[
                {
                    "role": "user",
                    "content": [
                        {"type": "text", "text": prompt},
                        {"type": "image_url", "image_url": {"url": f"data:image/jpeg;base64,{b64_image}"}}
                    ]
                }
            ],
            max_tokens=256,
        )

        content = response.choices[0].message.content.strip()
        label = self._parse_label(content)
        return LabelItem(id="", text=image_path, label=label, confidence=None, source="llm")

    def _parse_label(self, content: str) -> Optional[str]:
        for cat in self.categories:
            if cat in content:
                return cat
        return None

    def label_batch(
        self,
        image_paths: list[str],
        instruction: str = "",
        show_progress: bool = True
    ) -> list[LabelItem]:
        results = []
        for i, path in enumerate(image_paths):
            item = self.label(path, instruction)
            item.id = f"llm_{i}"
            results.append(item)
            if show_progress and (i + 1) % 20 == 0:
                print(f"[ImageLLMLabeler] 进度：{i + 1}/{len(image_paths)}")
        return results
