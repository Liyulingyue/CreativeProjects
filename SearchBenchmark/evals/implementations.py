"""Reference implementations for Search API benchmarks.

This module provides:
- RAG Agent: OpenAI-based answer synthesis
- Grader: OpenAI-based answer grading (uses official GRADER_TEMPLATE)
- Searcher: HTTP-based search API wrapper (supports queri, and any HTTP search API)

Example usage:
    from evals.implementations import HTTPSearcher, OpenAIRAGAgent, OpenAIGrader
    from evals.simpleqa import SimpleQAEvaluator

    searcher = HTTPSearcher(
        api_url="https://api.queri.com/search",
        api_key="your-api-key",
        name="queria",
        query_param="q",
        results_path="results",
    )
    rag_agent = OpenAIRAGAgent(model="gpt-4o-mini")
    grader = OpenAIGrader(model="gpt-4o")

    evaluator = SimpleQAEvaluator(searcher, rag_agent, grader)
"""

import json
import re
from typing import Any

import tiktoken
from openai import AsyncOpenAI

from .simpleqa import (
    BaseGrader,
    BaseRAGAgent,
    BaseSearcher,
    GRADER_TEMPLATE,
    RAGResult,
    SearchResult,
)

_TIKTOKEN_ENC = tiktoken.get_encoding("o200k_base")


class OpenAIRAGAgent(BaseRAGAgent):
    def __init__(
        self,
        model: str = "gpt-4o-mini",
        api_key: str | None = None,
        api_url: str | None = None,
        system_prompt: str | None = None,
    ):
        super().__init__(model=model, api_key=api_key)
        self._client = AsyncOpenAI(api_key=api_key, base_url=api_url)
        self._system_prompt = system_prompt or """\
You are a helpful assistant that answers questions based on search results.
Use ONLY the information from the search results to answer the question.
If the search results don't contain the answer, say "I don't know".
Be concise and cite your sources when possible.
"""

    async def synthesize(self, question: str, search_results: list[SearchResult]) -> RAGResult:
        formatted_results = []
        for i, r in enumerate(search_results, start=1):
            content = r.content or r.text or ""
            formatted_results.append(f"[{i}] {r.title}\nURL: {r.url}\n{content}")

        search_text = "\n\n".join(formatted_results)

        user_prompt = f"""\
Question: {question}

Search results:
{search_text}

Answer the question using ONLY the search results above. If they don't contain the answer, say "I don't know"."""

        response = await self._client.chat.completions.create(
            model=self.model,
            messages=[
                {"role": "system", "content": self._system_prompt},
                {"role": "user", "content": user_prompt},
            ],
            temperature=0.0,
        )

        answer = response.choices[0].message.content or "I don't know"

        return RAGResult(
            answer=answer,
            citations=search_results,
        )


class OpenAIGrader(BaseGrader):
    def __init__(
        self,
        model: str = "gpt-4o",
        api_key: str | None = None,
        api_url: str | None = None,
        temperature: float = 0.0,
    ):
        super().__init__(model=model, api_key=api_key, api_url=api_url, temperature=temperature)

    async def grade(
        self,
        query: str,
        expected_answer: str,
        predicted_answer: str,
    ) -> dict[str, Any]:
        client = AsyncOpenAI(api_key=self.api_key, base_url=self.api_url)

        grader_prompt = GRADER_TEMPLATE.format(
            question=query,
            target=expected_answer,
            predicted_answer=predicted_answer,
        )

        try:
            response = await client.chat.completions.create(
                model=self.model,
                messages=[
                    {"role": "user", "content": grader_prompt},
                ],
                temperature=self.temperature,
            )

            grading_response = response.choices[0].message.content or ""
            match = re.search(r"(A|B|C)", grading_response)
            grade_letter = match.group(0) if match else "C"

            is_correct = grade_letter == "A"
            is_incorrect = grade_letter == "B"
            is_not_attempted = grade_letter == "C"

            return {
                "grade_letter": grade_letter,
                "grading_response": grading_response,
                "is_correct": is_correct,
                "is_incorrect": is_incorrect,
                "is_not_attempted": is_not_attempted,
            }
        except Exception as e:
            import logging
            logging.getLogger(__name__).warning(f"Grading failed: {e}")
            return {
                "grade_letter": "C",
                "grading_response": "",
                "is_correct": False,
                "is_incorrect": False,
                "is_not_attempted": True,
            }


class HTTPSearcher(BaseSearcher):
    """Generic HTTP-based searcher that calls an external API.

    This is the main searcher implementation for queri and other HTTP-based search APIs.

    Args:
        api_url: The full URL endpoint for the search API
        api_key: API key for authentication (or use env var via "env:VAR_NAME")
        name: Identifier for this searcher (e.g., "queria", "serpapi")
        headers: Additional HTTP headers
        query_param: Name of the query parameter (default: "q")
        num_results_param: Name of the num_results parameter (default: "num_results")
        results_path: JSONPath to results array in response (e.g., "results" or "data.items")
        url_field: Field name for URL in result objects (default: "url")
        title_field: Field name for title (default: "title")
        text_field: Field name for text/snippet (default: "text")
    """

    def __init__(
        self,
        api_url: str,
        api_key: str | None = None,
        name: str = "http",
        headers: dict[str, str] | None = None,
        query_param: str = "q",
        num_results_param: str = "num_results",
        results_path: str | None = None,
        url_field: str = "url",
        title_field: str = "title",
        text_field: str = "text",
    ):
        self.api_url = api_url
        self.api_key = api_key
        self.name = name
        self._headers = headers or {}
        if api_key:
            if api_key.startswith("env:"):
                import os
                env_var = api_key[4:]
                self._headers["Authorization"] = f"Bearer {os.environ.get(env_var, '')}"
            else:
                self._headers["Authorization"] = f"Bearer {api_key}"
        self._query_param = query_param
        self._num_results_param = num_results_param
        self._results_path = results_path
        self._url_field = url_field
        self._title_field = title_field
        self._text_field = text_field
        self._client = None

    async def search(self, query: str, num_results: int = 10) -> list[SearchResult]:
        import httpx

        if self._client is None:
            self._client = httpx.AsyncClient(headers=self._headers, timeout=30.0)

        params = {
            self._query_param: query,
            self._num_results_param: num_results,
        }

        try:
            response = await self._client.get(self.api_url, params=params)
            response.raise_for_status()
            data = response.json()

            if self._results_path:
                for key in self._results_path.split("."):
                    data = data.get(key, data)

            results = []
            if isinstance(data, list):
                for item in data[:num_results]:
                    if isinstance(item, dict):
                        results.append(SearchResult(
                            url=item.get(self._url_field, ""),
                            title=item.get(self._title_field, ""),
                            text=item.get(self._text_field, ""),
                            highlights=item.get("highlights", []),
                            metadata=item,
                        ))
            return results
        except Exception as e:
            import logging
            logging.getLogger(__name__).warning(f"Search failed: {e}")
            return []


# Example: Reference implementation for Exa API (for reference only)
class ExaSearcherReference(BaseSearcher):
    """Reference implementation for Exa Search API.

    NOTE: This is for reference only. Use HTTPSearcher for actual API calls
    or implement your own searcher for production use.
    """

    def __init__(
        self,
        api_key: str | None = None,
        search_type: str = "fast",
    ):
        self.api_key = api_key
        self.search_type = search_type
        self.name = "exa-reference"
        self._client = None

    async def search(self, query: str, num_results: int = 10) -> list[SearchResult]:
        import httpx

        if self._client is None:
            headers = {"x-api-key": self.api_key} if self.api_key else {}
            self._client = httpx.AsyncClient(headers=headers, timeout=30.0)

        try:
            response = await self._client.post(
                "https://api.exa.ai/search",
                json={
                    "query": query,
                    "type": self.search_type,
                    "num_results": num_results,
                    "contents": {"text": True, "highlights": True},
                },
            )
            response.raise_for_status()
            data = response.json()

            results = []
            for item in data.get("results", []):
                results.append(SearchResult(
                    url=item.get("url", ""),
                    title=item.get("title", ""),
                    text=item.get("text", ""),
                    highlights=item.get("highlights", []),
                    metadata=item,
                ))
            return results
        except Exception as e:
            import logging
            logging.getLogger(__name__).warning(f"Exa search failed: {e}")
            return []


class FreshQAGrader(BaseGrader):
    """FreshQA Grader using official FreshEval prompts.

    Reference: references/freshqa/fresheval_relaxed.ipynb
    """

    FRESHEVAL_RELAXED_PREFIX = """\
Please evaluate the response to a question under relaxed evaluation, where\
 hallucinations, outdated information, and ill-formed answers are allowed,\
 as long as the primary answer is accurate. Please credit the response\
 only if it provides a confident and definitive answer, or the correct\
 answer can be obviously inferred from the response. The primary or final\
 answer when standing alone must be accurate. Any additional information\
 that is provided must not contradict the primary answer or reshape one's\
 perception of it. For false-premise questions, the response must point\
 out the presence of a false premise to receive credit. For answers that\
 involve names of entities (e.g., people), complete names or commonly\
 recognized names are expected. Regarding numerical answers, approximate\
 numbers are generally not accepted unless explicitly included in the\
 ground-truth answers. We accept ill-formed responses (including those in\
 a non-English language), as well as hallucinated or outdated information\
 that does not significantly impact the primary answer."""

    FRESHEVAL_STRICT_PREFIX = """\
Please evaluate the response to a question under strict evaluation, where\
 the response must be factual, up-to-date, and well-formed. Any hallucination,\
 outdated information, or ill-formed answer will result in a incorrect rating.\
 The primary or final answer must be accurate and current. For false-premise\
 questions, the response must point out the presence of a false premise.\
 For answers involving entity names, complete names are expected.\
 For numerical answers, the answer must be exact unless explicitly marked as approximate\
 in the ground-truth. Responses must be well-formed and in the correct language."""

    DEMO_EXAMPLES_RELAXED = [
        {
            "question": "How old is the world's oldest verified living person?",
            "answer": "117 years old",
            "response": "As of today, the oldest verified living person is Maria Branyas Morera, born March 4, 1907, making her 117 years old.",
            "evaluation": "correct",
        },
        {
            "question": "When did the UK adopt the Euro?",
            "answer": "The UK has never adopted the Euro.",
            "response": "The UK has never adopted the Euro as its official currency.",
            "evaluation": "correct",
        },
    ]

    async def grade(
        self,
        question: str,
        answers: list[str],
        predicted_answer: str,
        mode: str = "relaxed",
    ) -> dict[str, Any]:
        client = AsyncOpenAI(api_key=self.api_key, base_url=self.api_url)

        prefix = self.FRESHEVAL_RELAXED_PREFIX if mode == "relaxed" else self.FRESHEVAL_STRICT_PREFIX

        answers_str = " | ".join(f"[{a}]" for a in answers[:3])

        grader_prompt = f"""\
{prefix}

question: {question}
ground-truth answers: {answers_str}
response: {predicted_answer}

evaluation:"""

        try:
            response = await client.chat.completions.create(
                model=self.model,
                messages=[{"role": "user", "content": grader_prompt}],
                temperature=self.temperature,
                max_tokens=256,
            )

            grading_response = response.choices[0].message.content or ""

            is_correct = False
            if "correct" in grading_response.lower() or "true" in grading_response.lower():
                if "not correct" not in grading_response.lower() and "false" not in grading_response.lower():
                    is_correct = True

            return {
                "grading_response": grading_response,
                "is_correct": is_correct,
            }
        except Exception as e:
            import logging
            logging.getLogger(__name__).warning(f"FreshQA grading failed: {e}")
            return {"grading_response": "", "is_correct": False}


class FinSearchCompGrader(BaseGrader):
    """FinSearchComp Grader using official judge prompts.

    Reference: references/FinSearchComp/finsearchcomp/eval/eval.py
    Official template is in Chinese, used for T2 (static) questions.
    """

    JUDGE_SYSTEM_PROMPT = """\
你是一个智慧的金融问题答案的判断和评分者。你会收到一道<题目>、该题目的<标准答案>和一个<学生答案>，有些<标准答案>中附有"评分要点"作为补充。你需要对<学生答案>进行判断，完成以下任务：
1. 根据<学生答案>的内容，准确地识别出它的最终答案（只需要识别，不需要输出）。你可以通过分析<学生答案>来识别最终答案的位置和内容，也可以通过检索关键词来识别最终答案的位置和内容，关键词包括但不限于"答案为""最终结果是""正确选项是"等。如果<学生答案>为空，即<学生答案>没有任何内容，则直接给0分，并跳过下面的第2步和第3步。
2. 分别列出<标准答案>和你识别出的<学生答案>的最终答案，并将二者进行比对（不需要输出列举和比对的过程或结果）。
3. 根据比对的结果和<标准答案>中可能附有的评分要点，判断<学生答案>是否正确并评分。分数只有1分和0分，1分表示<学生答案>正确，0分表示<学生答案>错误，没有除0和1之外的其他分数。
注意：
1. 你不需要也不应该自行回答或解决问题，你的唯一任务是判断并评分。
2. <标准答案>是准确无误的，你可以完全信任它。
3. 如果<标准答案>包含2个"""

    JUDGE_PROMPT_TEMPLATE = """<题目>：
{prompt}

<标准答案>：
{response_reference}

<学生答案>：
{response}"""

    async def grade(
        self,
        question: str,
        ground_truth: str,
        predicted_answer: str,
        reference: str = "",
    ) -> dict[str, Any]:
        client = AsyncOpenAI(api_key=self.api_key, base_url=self.api_url)

        grader_prompt = self.JUDGE_PROMPT_TEMPLATE.format(
            prompt=question,
            response_reference=reference or ground_truth,
            response=predicted_answer,
        )

        try:
            response = await client.chat.completions.create(
                model=self.model,
                messages=[
                    {"role": "system", "content": self.JUDGE_SYSTEM_PROMPT},
                    {"role": "user", "content": grader_prompt},
                ],
                temperature=self.temperature,
            )

            grading_response = response.choices[0].message.content or ""

            score = 0.0
            if "1" in grading_response or "正确" in grading_response:
                score = 1.0

            return {
                "grading_response": grading_response,
                "score": score,
            }
        except Exception as e:
            import logging
            logging.getLogger(__name__).warning(f"FinSearchComp grading failed: {e}")
            return {"grading_response": "", "score": 0.0}
