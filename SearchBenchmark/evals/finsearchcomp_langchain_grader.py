"""FinSearchComp LangChain graders."""

from typing import Any

from langchain_openai import ChatOpenAI

LANGCHAIN_GRADING_SYSTEM = """\
You are evaluating financial search and reasoning answers.
Evaluate whether the predicted answer is correct based on the reference answer."""


LANGCHAIN_GRADING_USER = """\
Question: {question}
Reference Answer: {reference}
Predicted Answer: {predicted}

Score the predicted answer from 0.0 to 1.0 based on correctness.
Return just the numeric score."""


class FinSearchCompLangChainGrader:
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
        self.api_url = api_url

    async def grade(
        self,
        question: str,
        ground_truth: str,
        predicted_answer: str,
        reference: str = "",
    ) -> dict[str, Any]:
        llm = ChatOpenAI(
            model=self.model,
            api_key=self.api_key,
            base_url=self.api_url,
            temperature=self.temperature,
        )
        response = await llm.ainvoke([
            {"role": "system", "content": LANGCHAIN_GRADING_SYSTEM},
            {"role": "user", "content": LANGCHAIN_GRADING_USER.format(
                question=question,
                reference=reference,
                predicted=predicted_answer,
            )},
        ])
        content = response.content.strip()

        try:
            score = float(content)
            score = max(0.0, min(1.0, score))
        except ValueError:
            score = 0.0

        return {"score": score, "raw_response": content}
