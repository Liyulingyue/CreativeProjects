"""FinSearchComp eval — 支持多种模式.

Modes:
    simple: 直接 OpenAI SDK，简化 prompt
    langchain: LangChain 版本，对齐 Tavily/web-search-api-evals
"""

import argparse
import asyncio
import json
import logging
import time
from pathlib import Path
from typing import Any

from rich.console import Console
from rich.progress import BarColumn, Progress, TextColumn, TimeElapsedColumn
from rich.table import Table

console = Console()
logger = logging.getLogger(__name__)
DEFAULT_DATA_DIR = Path(__file__).parent.parent / "data"


class BaseSearcher:
    name: str = "base"

    async def search(self, query: str, num_results: int = 10) -> list[dict]:
        raise NotImplementedError


class BaseRAGAgent:
    def __init__(self, model: str = "gpt-4o-mini", api_key: str | None = None, api_url: str | None = None):
        self.model = model
        self.api_key = api_key
        self.api_url = api_url

    async def synthesize(self, question: str, search_results: list[dict]) -> dict:
        raise NotImplementedError


class BaseGrader:
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
        raise NotImplementedError


def load_finsearchcomp(json_path: Path | None = None) -> list[dict]:
    if json_path is None:
        json_path = DEFAULT_DATA_DIR / "finsearchcomp" / "finsearchcomp_data.json"
    if not json_path.exists():
        json_path = DEFAULT_DATA_DIR / "finsearchcomp_data.json"

    if not json_path.exists():
        console.print(f"[yellow]File not found: {json_path}[/yellow]")
        return []

    with open(json_path, encoding="utf-8") as f:
        data = json.load(f)

    if isinstance(data, list):
        return data
    return [data] if data else []


def print_info():
    items = load_finsearchcomp()
    console.print("\n[bold]FinSearchComp Dataset[/bold]")
    console.print("  Financial search and reasoning benchmark.\n")

    if items:
        console.print(f"  Total items: {len(items)}")
        if len(items) > 0:
            item = items[0]
            console.print(f"  Sample prompt: {item.get('prompt', '')[:80]}...")
            console.print(f"  prompt_id: {item.get('prompt_id', '')}")
    else:
        console.print("  [yellow]No data loaded.[/yellow]")
    console.print()


class FinSearchCompEvaluator:
    def __init__(
        self,
        searcher: BaseSearcher,
        rag_agent: BaseRAGAgent,
        grader: BaseGrader,
    ):
        self.searcher = searcher
        self.rag_agent = rag_agent
        self.grader = grader

    async def evaluate(
        self,
        items: list[dict],
        num_results: int = 5,
        concurrency: int = 10,
        verbose: bool = True,
    ) -> list[dict]:
        semaphore = asyncio.Semaphore(concurrency)
        results = []

        if verbose:
            console.print(f"[bold]Evaluating {len(items)} FinSearchComp items[/bold]")
            console.print(f"  Searcher: {self.searcher.name}")
            console.print(f"  RAG Model: {self.rag_agent.model}")
            console.print(f"  Grader Model: {self.grader.model}\n")

        with Progress(
            TextColumn("[cyan]{task.description:>20}[/cyan]"),
            BarColumn(),
            TextColumn("[progress.percentage]{task.percentage:>3.0f}%"),
            TextColumn("{task.completed}/{task.total}"),
            TimeElapsedColumn(),
            console=console,
            disable=not verbose,
        ) as progress:
            task_id = progress.add_task("Evaluating", total=len(items))

            async def process(item: dict) -> dict:
                async with semaphore:
                    prompt = item.get("prompt", "")
                    prompt_id = item.get("prompt_id", [""])[0] if isinstance(item.get("prompt_id"), list) else item.get("prompt_id", "")
                    reference = item.get("response_reference", "")
                    start = time.time()

                    try:
                        search_results = await self.searcher.search(prompt, num_results=num_results)
                    except Exception as e:
                        logger.warning(f"Search failed: {e}")
                        search_results = []

                    latency = time.time() - start

                    try:
                        rag_result = await self.rag_agent.synthesize(prompt, search_results)
                        predicted = rag_result.get("answer", "")
                    except Exception as e:
                        logger.warning(f"RAG synthesis failed: {e}")
                        predicted = "I don't know"

                    try:
                        grade = await self.grader.grade(
                            question=prompt,
                            ground_truth="",
                            predicted_answer=predicted,
                            reference=reference,
                        )
                        score = grade.get("score", 0.0)
                    except Exception as e:
                        logger.warning(f"Grading failed: {e}")
                        score = 0.0

                    progress.advance(task_id)

                    return {
                        "id": prompt_id,
                        "prompt": prompt,
                        "predicted_answer": predicted,
                        "score": score,
                        "latency": round(latency, 2),
                    }

            results = await asyncio.gather(*[process(item) for item in items])

        return list(results)


def print_results(all_results: list[dict]):
    if not all_results:
        return

    n = len(all_results)
    total_score = sum(r.get("score", 0.0) for r in all_results)
    avg_score = total_score / n if n > 0 else 0.0
    valid_scores = sum(1 for r in all_results if r.get("score", -100000) > -1000)
    accuracy = total_score / valid_scores if valid_scores > 0 else 0.0

    table = Table(title="FinSearchComp Results")
    table.add_column("Metric", style="cyan")
    table.add_column("Value", justify="right")

    table.add_row("Total Items", str(n))
    table.add_row("Valid Evaluations", str(valid_scores))
    table.add_row("Average Score", f"{avg_score:.4f}")
    table.add_row("Accuracy", f"{accuracy:.1%}")

    console.print()
    console.print(table)


async def run(
    searcher: BaseSearcher,
    rag_agent: BaseRAGAgent,
    grader: BaseGrader,
    limit: int | None = None,
    output: str | None = None,
    num_results: int = 5,
    concurrency: int = 10,
    json_path: str | None = None,
    verbose: bool = True,
) -> list[dict]:
    items = load_finsearchcomp(Path(json_path) if json_path else None)

    if not items:
        console.print("[red]No items found. Ensure data/finsearchcomp/ exists.[/red]")
        return []

    if limit:
        items = items[:limit]

    evaluator = FinSearchCompEvaluator(searcher, rag_agent, grader)
    results = await evaluator.evaluate(
        items=items,
        num_results=num_results,
        concurrency=concurrency,
        verbose=verbose,
    )

    if verbose:
        print_results(results)

    if output:
        with open(output, "w") as f:
            json.dump(results, f, indent=2)
        console.print(f"\n[green]Results saved to {output}[/green]")

    return results


def main():
    parser = argparse.ArgumentParser(description="FinSearchComp eval")
    parser.add_argument("--info", action="store_true", help="Print dataset info")
    parser.add_argument("--mode", default="simple", choices=["simple", "langchain"], help="Evaluation mode")
    parser.add_argument("--searcher-api-url", help="Search API URL")
    parser.add_argument("--searcher-api-key", default=None, help="Search API key")
    parser.add_argument("--rag-model", default="gpt-4o-mini", help="RAG model")
    parser.add_argument("--rag-api-key", default=None, help="RAG API key")
    parser.add_argument("--grader-model", default="gpt-4o", help="Grader model")
    parser.add_argument("--grader-api-key", default=None, help="Grader API key")
    parser.add_argument("--limit", type=int, help="Limit items")
    parser.add_argument("--num-results", type=int, default=5, help="Search results")
    parser.add_argument("--concurrency", type=int, default=10)
    parser.add_argument("--output", "-o", help="Output file")
    parser.add_argument("--json-path", help="Path to FinSearchComp JSON")
    args = parser.parse_args()

    if args.info:
        print_info()
        return

    if not args.searcher_api_url:
        console.print("[red]--searcher-api-url is required[/red]")
        return

    if args.mode == "simple":
        from .simple_impl import SimpleSearcher, SimpleRAGAgent
        from .finsearchcomp_simple_grader import FinSearchCompSimpleGrader

        searcher = SimpleSearcher(args.searcher_api_url, args.searcher_api_key)
        rag_agent = SimpleRAGAgent(args.rag_model, api_key=args.rag_api_key)
        grader = FinSearchCompSimpleGrader(args.grader_model, api_key=args.grader_api_key)
    elif args.mode == "langchain":
        from .langchain_impl import LangChainSearcher, LangChainRAGAgent
        from .finsearchcomp_langchain_grader import FinSearchCompLangChainGrader

        searcher = LangChainSearcher(args.searcher_api_url, args.searcher_api_key)
        rag_agent = LangChainRAGAgent(args.rag_model, api_key=args.rag_api_key)
        grader = FinSearchCompLangChainGrader(args.grader_model, api_key=args.grader_api_key)

    asyncio.run(run(
        searcher=searcher,
        rag_agent=rag_agent,
        grader=grader,
        limit=args.limit,
        output=args.output,
        num_results=args.num_results,
        concurrency=args.concurrency,
        json_path=args.json_path,
    ))


if __name__ == "__main__":
    main()
