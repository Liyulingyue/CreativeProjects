"""FreshQA LangChain graders."""

from typing import Any

from langchain_openai import ChatOpenAI

LANGCHAIN_GRADING_SYSTEM_RELAXED = """\
You are evaluating the factuality of a model's answer to a question.
The question may have a false premise that the model should reject.
Evaluate whether the answer is factually correct based on the ground truth answers provided.

Mode: RELAXED
- A answer is correct if it contains the correct information
- Minor paraphrasing is acceptable
- If the question has a false premise, saying "I cannot answer" is acceptable
"""

LANGCHAIN_GRADING_SYSTEM_STRICT = """\
You are evaluating the factuality of a model's answer to a question.
The question may have a false premise that the model should reject.
Evaluate whether the answer is factually correct based on the ground truth answers provided.

Mode: STRICT
- A answer is correct only if it is precise and complete
- Partial information is not acceptable
- If the question has a false premise, saying "I cannot answer" is NOT acceptable
"""

LANGCHAIN_GRADING_USER = """\
Question: {question}
Ground Truth Answers: {answers}
Model's Answer: {predicted}

Is the model's answer correct? Answer YES or NO."""


class FreshQALangChainGrader:
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
        answers: list[str],
        predicted_answer: str,
        mode: str = "relaxed",
    ) -> dict[str, Any]:
        system_prompt = (
            LANGCHAIN_GRADING_SYSTEM_RELAXED if mode == "relaxed" else LANGCHAIN_GRADING_SYSTEM_STRICT
        )
        user_prompt = LANGCHAIN_GRADING_USER.format(
            question=question,
            answers=", ".join(answers),
            predicted=predicted_answer,
        )

        llm = ChatOpenAI(
            model=self.model,
            api_key=self.api_key,
            base_url=self.api_url,
            temperature=self.temperature,
        )
        response = await llm.ainvoke([{"role": "system", "content": system_prompt}, {"role": "user", "content": user_prompt}])
        content = response.content.strip()

        is_correct = content.upper() in ("YES", "Y")
        return {"is_correct": is_correct, "raw_response": content}
