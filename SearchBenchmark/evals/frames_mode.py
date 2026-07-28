"""FRAMES eval — 支持多种模式.

Modes:
    simple: 直接 OpenAI SDK，简化 prompt
    langchain: LangChain 版本，对齐 Tavily/web-search-api-evals
"""

import argparse
import asyncio
import csv
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
        answer: str,
        predicted_answer: str,
    ) -> dict[str, Any]:
        raise NotImplementedError


def load_frames(tsv_path: Path | None = None) -> list[dict]:
    if tsv_path is None:
        tsv_path = DEFAULT_DATA_DIR / "frames" / "frames-benchmark.tsv"

    if not tsv_path.exists():
        console.print(f"[yellow]File not found: {tsv_path}[/yellow]")
        return []

    questions = []
    with open(tsv_path, newline="", encoding="utf-8") as f:
        reader = csv.DictReader(f, delimiter="\t")
        for row in reader:
            prompt = row.get("Prompt", "").strip()
            answer = row.get("Answer", "").strip()
            if not prompt:
                continue
            wiki_links = []
            for i in range(1, 12):
                key = f"wikipedia_link_{i}"
                if key in row and row[key]:
                    wiki_links.append(row[key])
            wiki_links_str = row.get("wiki_links", "")
            if wiki_links_str:
                try:
                    wiki_links = json.loads(wiki_links_str)
                except:
                    pass
            questions.append({
                "id": row.get("Unnamed: 0", ""),
                "prompt": prompt,
                "answer": answer,
                "wiki_links": wiki_links,
                "reasoning_types": row.get("reasoning_types", ""),
            })
    return questions


def print_info():
    questions = load_frames()
    console.print("\n[bold]FRAMES Dataset[/bold]")
    console.print("  Multi-hop reasoning over Wikipedia.\n")

    if questions:
        console.print(f"  Total questions: {len(questions)}")
        console.print(f"  Sample question: {questions[0].get('prompt', '')[:80]}...")
        console.print(f"  Sample answer: {questions[0].get('answer', '')}")
    else:
        console.print("  [yellow]No questions loaded.[/yellow]")
    console.print()


class FRAMESEvaluator:
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
        questions: list[dict],
        num_results: int = 5,
        concurrency: int = 10,
        verbose: bool = True,
    ) -> list[dict]:
        semaphore = asyncio.Semaphore(concurrency)
        results = []

        if verbose:
            console.print(f"[bold]Evaluating {len(questions)} FRAMES questions[/bold]")
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
            task_id = progress.add_task("Evaluating", total=len(questions))

            async def process(q: dict) -> dict:
                async with semaphore:
                    prompt = q.get("prompt", "")
                    answer = q.get("answer", "")
                    question_id = q.get("id", "")
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
                            answer=answer,
                            predicted_answer=predicted,
                        )
                        is_correct = grade.get("is_correct", False)
                    except Exception as e:
                        logger.warning(f"Grading failed: {e}")
                        is_correct = False

                    progress.advance(task_id)

                    return {
                        "id": question_id,
                        "prompt": prompt,
                        "answer": answer,
                        "predicted_answer": predicted,
                        "is_correct": is_correct,
                        "latency": round(latency, 2),
                    }

            results = await asyncio.gather(*[process(q) for q in questions])

        return list(results)


def print_results(all_results: list[dict]):
    if not all_results:
        return

    n = len(all_results)
    correct = sum(1 for r in all_results if r.get("is_correct", False))

    table = Table(title="FRAMES Results")
    table.add_column("Metric", style="cyan")
    table.add_column("Value", justify="right")

    table.add_row("Total Questions", str(n))
    table.add_row("Correct", str(correct))
    table.add_row("Accuracy", f"{correct / n:.1%}" if n > 0 else "N/A")

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
    tsv_path: str | None = None,
    verbose: bool = True,
) -> list[dict]:
    questions = load_frames(Path(tsv_path) if tsv_path else None)

    if not questions:
        console.print("[red]No questions found. Ensure data/frames/ exists.[/red]")
        return []

    if limit:
        questions = questions[:limit]

    evaluator = FRAMESEvaluator(searcher, rag_agent, grader)
    results = await evaluator.evaluate(
        questions=questions,
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
    parser = argparse.ArgumentParser(description="FRAMES eval")
    parser.add_argument("--info", action="store_true", help="Print dataset info")
    parser.add_argument("--mode", default="simple", choices=["simple", "langchain"], help="Evaluation mode")
    parser.add_argument("--searcher-api-url", help="Search API URL")
    parser.add_argument("--searcher-api-key", default=None, help="Search API key")
    parser.add_argument("--rag-model", default="gpt-4o-mini", help="RAG model")
    parser.add_argument("--rag-api-key", default=None, help="RAG API key")
    parser.add_argument("--grader-model", default="gpt-4o", help="Grader model")
    parser.add_argument("--grader-api-key", default=None, help="Grader API key")
    parser.add_argument("--limit", type=int, help="Limit questions")
    parser.add_argument("--num-results", type=int, default=5, help="Search results")
    parser.add_argument("--concurrency", type=int, default=10)
    parser.add_argument("--output", "-o", help="Output file")
    parser.add_argument("--tsv-path", help="Path to FRAMES TSV")
    args = parser.parse_args()

    if args.info:
        print_info()
        return

    if not args.searcher_api_url:
        console.print("[red]--searcher-api-url is required[/red]")
        return

    if args.mode == "simple":
        from .simple_impl import SimpleSearcher, SimpleRAGAgent
        from .frames_simple_grader import FRAMESSimpleGrader

        searcher = SimpleSearcher(args.searcher_api_url, args.searcher_api_key)
        rag_agent = SimpleRAGAgent(args.rag_model, api_key=args.rag_api_key)
        grader = FRAMESSimpleGrader(args.grader_model, api_key=args.grader_api_key)
    elif args.mode == "langchain":
        from .langchain_impl import LangChainSearcher, LangChainRAGAgent
        from .frames_langchain_grader import FRAMESLangChainGrader

        searcher = LangChainSearcher(args.searcher_api_url, args.searcher_api_key)
        rag_agent = LangChainRAGAgent(args.rag_model, api_key=args.rag_api_key)
        grader = FRAMESLangChainGrader(args.grader_model, api_key=args.grader_api_key)

    asyncio.run(run(
        searcher=searcher,
        rag_agent=rag_agent,
        grader=grader,
        limit=args.limit,
        output=args.output,
        num_results=args.num_results,
        concurrency=args.concurrency,
        tsv_path=args.tsv_path,
    ))


if __name__ == "__main__":
    main()
