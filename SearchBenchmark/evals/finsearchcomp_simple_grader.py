"""FinSearchComp Simple graders."""

import aiohttp
from typing import Any

SIMPLE_GRADING_SYSTEM = """\
You are evaluating financial search and reasoning answers.
Evaluate whether the predicted answer is correct based on the reference answer."""


SIMPLE_GRADING_USER = """\
Question: {question}
Reference Answer: {reference}
Predicted Answer: {predicted}

Score the predicted answer from 0.0 to 1.0 based on correctness.
Return just the numeric score."""


class FinSearchCompSimpleGrader:
    def __init__(
        self,
        model: str = "gpt-4o",
        temperature: float = 0.0,
        api_key: str | None = None,
        api_url: str | None = None,
    ):
        self.model = model
        self.temperature = temperature
        self.api_key = api_key
        self.api_url = api_url or "https://api.openai.com/v1/chat/completions"

    async def grade(
        self,
        question: str,
        ground_truth: str,
        predicted_answer: str,
        reference: str = "",
    ) -> dict[str, Any]:
        messages = [
            {"role": "system", "content": SIMPLE_GRADING_SYSTEM},
            {"role": "user", "content": SIMPLE_GRADING_USER.format(
                question=question,
                reference=reference,
                predicted=predicted_answer,
            )},
        ]

        headers = {
            "Content-Type": "application/json",
            "Authorization": f"Bearer {self.api_key}",
        }
        payload = {
            "model": self.model,
            "messages": messages,
            "temperature": self.temperature,
        }

        async with aiohttp.ClientSession() as session:
            async with session.post(
                self.api_url,
                headers=headers,
                json=payload,
                timeout=aiohttp.ClientTimeout(total=30),
            ) as resp:
                data = await resp.json()
                content = data.get("choices", [{}])[0].get("message", {}).get("content", "").strip()

        try:
            score = float(content)
            score = max(0.0, min(1.0, score))
        except ValueError:
            score = 0.0

        return {"score": score, "raw_response": content}
