"""LangChain implementations for alignment with Tavily/web-search-api-evals."""

from typing import Any

from langchain_core.runnables import RunnablePassthrough
from langchain_core.output_parser import StrOutputParser
from langchain_openai import ChatOpenAI


class LangChainSearcher:
    name = "langchain"

    def __init__(self, api_url: str, api_key: str | None = None):
        self.api_url = api_url
        self.api_key = api_key

    async def search(self, query: str, num_results: int = 10) -> list[dict]:
        from langchain_community.tools.dalle_image_generator import dalle_tool
        from langchain_community.utilities.dalle_image_generator import DallETool
        from langchain_community.tools.tavily_search import TavilySearchResults

        tool = TavilySearchResults(api_key=self.api_key, max_results=num_results)
        results = await tool.ainvoke({"query": query})
        return results


class LangChainRAGAgent:
    def __init__(self, model: str = "gpt-4o-mini", api_key: str | None = None, api_url: str | None = None):
        self.model = model
        self.api_key = api_key
        self.api_url = api_url

    async def synthesize(self, question: str, search_results: list[dict]) -> dict:
        context = "\n\n".join(
            f"Source {i+1}: {r.get('title', 'Untitled')}\n{r.get('snippet', r.get('content', ''))}"
            for i, r in enumerate(search_results)
        )

        prompt = f"""You are a helpful assistant. Answer based on the provided search results.

Question: {question}

Search Results:
{context}

Answer:"""

        llm = ChatOpenAI(
            model=self.model,
            api_key=self.api_key,
            base_url=self.api_url,
            temperature=0.0,
        )
        chain = RunnablePassthrough() | StrOutputParser()
        answer = await chain.ainvoke(prompt)
        return {"answer": answer.strip()}


class LangChainGrader:
    def __init__(
        self,
        model: str = "gpt-4o",
        temperature: float = 0.0,
        api_key: str | None = None,
        api_url: str | None = None,
        grader_template: str | None = None,
    ):
        self.model = model
        self.temperature = temperature
        self.api_key = api_key
        self.api_url = api_url
        self.grader_template = grader_template or ""

    async def grade(self, query: str, expected_answer: str, predicted_answer: str) -> dict[str, Any]:
        prompt = self.grader_template.format(
            question=query,
            target=expected_answer,
            predicted_answer=predicted_answer,
        )

        llm = ChatOpenAI(
            model=self.model,
            api_key=self.api_key,
            base_url=self.api_url,
            temperature=self.temperature,
        )
        response = await llm.ainvoke(prompt)
        content = response.content.strip()

        is_correct = content in ("A", "CORRECT")
        return {"is_correct": is_correct, "raw_response": content}
