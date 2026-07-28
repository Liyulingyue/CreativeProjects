"""FRAMES LangChain graders."""

from typing import Any

from langchain_openai import ChatOpenAI

LANGCHAIN_GRADING_SYSTEM = """\
You are evaluating a model's answer to a multi-hop reasoning question.
The question requires reasoning over multiple Wikipedia articles.
Evaluate whether the predicted answer is correct based on the gold answer."""


LANGCHAIN_GRADING_USER = """\
Question: {question}
Gold Answer: {answer}
Predicted Answer: {predicted}

Is the predicted answer correct? Answer YES or NO."""


class FRAMESLangChainGrader:
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
        answer: str,
        predicted_answer: str,
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
                answer=answer,
                predicted=predicted_answer,
            )},
        ])
        content = response.content.strip()

        is_correct = content.upper() in ("YES", "Y")
        return {"is_correct": is_correct, "raw_response": content}
