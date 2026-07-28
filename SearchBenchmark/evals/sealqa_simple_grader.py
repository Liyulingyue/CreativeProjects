"""SealQA Simple graders."""

import aiohttp
from typing import Any

SIMPLE_GRADING_SYSTEM = """\
You are evaluating search-augmented language model answers on fact-seeking questions.

The questions in SealQA are challenging because:
1. Web search may yield conflicting, noisy, or unhelpful results
2. The answer requires reasoning over multiple sources

Evaluate whether the predicted answer is correct based on the evidence retrieved."""


SIMPLE_GRADING_USER = """\
Question: {question}
Evidence: {evidence}
Predicted Answer: {predicted}

Is the predicted answer correct? Answer YES or NO."""


class SealQASimpleGrader:
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
        evidence: str,
        predicted_answer: str,
    ) -> dict[str, Any]:
        messages = [
            {"role": "system", "content": SIMPLE_GRADING_SYSTEM},
            {"role": "user", "content": SIMPLE_GRADING_USER.format(
                question=question,
                evidence=evidence,
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

        is_correct = content.upper() in ("YES", "Y")
        return {"is_correct": is_correct, "raw_response": content}
