from typing import Optional
from openai import OpenAI
from openaijsonwrapper import OpenAIJsonWrapper

from src.base import BaseLabeler, LabelItem
from src.task_type import TaskType


class TextLLMLabeler(BaseLabeler):
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
        self._wrappers: dict[str, object] = {}

    @property
    def task_type(self) -> TaskType:
        return TaskType.TEXT_CLASSIFICATION

    def _get_wrapper(self, categories: list[str]):
        key = "|".join(sorted(categories))
        if key in self._wrappers:
            return self._wrappers[key]

        target = {
            "label": f"string ({' | '.join(categories)})",
            "reason": "string (简短标注理由)"
        }
        wrapper = OpenAIJsonWrapper(
            self.client,
            model=self.model_name,
            target_structure=target,
        )
        self._wrappers[key] = wrapper
        return wrapper

    def label(self, text: str, instruction: str = "") -> LabelItem:
        wrapper = self._get_wrapper(self.categories)
        result = wrapper.chat(
            messages=[
                {"role": "user", "content": f"{instruction}\n\n文本：{text}"}
            ],
            extra_requirements=[
                f"分类选项：{'、'.join(self.categories)}"
            ]
        )

        if result["error"]:
            return LabelItem(id="", text=text, label=None, confidence=None, source="llm")

        data = result["data"]
        return LabelItem(
            id="",
            text=text,
            label=data.get("label"),
            confidence=None,
            source="llm"
        )

    def label_batch(
        self,
        texts: list[str],
        instruction: str = "",
        show_progress: bool = True
    ) -> list[LabelItem]:
        results = []
        for i, text in enumerate(texts):
            item = self.label(text, instruction)
            item.id = f"llm_{i}"
            results.append(item)
            if show_progress and (i + 1) % 100 == 0:
                print(f"[TextLLMLabeler] 进度：{i + 1}/{len(texts)}")
        return results


LLMLabeler = TextLLMLabeler
