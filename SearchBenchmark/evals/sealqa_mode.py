"""SealQA eval — 支持多种模式.

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
        evidence: str,
        predicted_answer: str,
    ) -> dict[str, Any]:
        raise NotImplementedError


def load_sealqa(config: str = "seal-0", data_dir: Path | None = None) -> list[dict]:
    data_dir = data_dir or DEFAULT_DATA_DIR / "sealqa"

    parquet_path = data_dir / f"{config}.parquet"

    if not parquet_path.exists() or parquet_path.stat().st_size < 1000:
        console.print(f"[yellow]Parquet file not found or too small: {parquet_path}[/yellow]")
        console.print("  SealQA data needs to be downloaded from HuggingFace:")
        console.print("    from datasets import load_dataset")
        console.print("    ds = load_dataset('vtllms/sealqa', name='seal_0', split='test')")
        console.print("    ds.to_parquet('data/sealqa/seal-0.parquet')")
        return []

    try:
        import pandas as pd
        df = pd.read_parquet(parquet_path)
        questions = df.to_dict("records")
        return questions
    except Exception as e:
        logger.warning(f"Failed to load parquet: {e}")
        return []


def print_info():
    configs = ["seal-0", "seal-hard", "longseal"]
    console.print("\n[bold]SealQA Dataset[/bold]")
    console.print("  SEarch-Augmented Language models on challenging QA.\n")

    for config in configs:
        questions = load_sealqa(config)
        if questions:
            console.print(f"  {config}: {len(questions)} questions")
            if len(questions) > 0:
                console.print(f"    Sample: {questions[0].get('question', questions[0].get('input', ''))[:60]}...")
        else:
            console.print(f"  {config}: [yellow]data not loaded (git lfs placeholder)[/yellow]")
    console.print()


class SealQAEvaluator:
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
        use_retrieved_evidence: bool = True,
        verbose: bool = True,
    ) -> list[dict]:
        semaphore = asyncio.Semaphore(concurrency)
        results = []

        if verbose:
            console.print(f"[bold]Evaluating {len(questions)} SealQA questions[/bold]")
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
                    question_text = q.get("question", q.get("input", ""))
                    answer_text = q.get("answer", q.get("ground_truth", ""))
                    question_id = q.get("id", "")
                    start = time.time()

                    try:
                        search_results = await self.searcher.search(
                            question_text, num_results=num_results
                        )
                    except Exception as e:
                        logger.warning(f"Search failed: {e}")
                        search_results = []

                    latency = time.time() - start

                    if use_retrieved_evidence:
                        try:
                            rag_result = await self.rag_agent.synthesize(question_text, search_results)
                            predicted = rag_result.get("answer", "")
                        except Exception as e:
                            logger.warning(f"RAG synthesis failed: {e}")
                            predicted = "I don't know"
                    else:
                        predicted = "Not evaluated (w/o search mode)"

                    try:
                        evidence = "\n".join([
                            f"- {r.get('title', '')}: {r.get('text', r.get('snippet', ''))}"
                            for r in search_results[:3]
                        ])
                        grade = await self.grader.grade(
                            question=question_text,
                            evidence=evidence,
                            predicted_answer=predicted,
                        )
                        is_correct = grade.get("is_correct", False)
                    except Exception as e:
                        logger.warning(f"Grading failed: {e}")
                        is_correct = False

                    progress.advance(task_id)

                    return {
                        "id": question_id,
                        "query": question_text,
                        "answer": answer_text,
                        "predicted_answer": predicted,
                        "is_correct": is_correct,
                        "latency": round(latency, 2),
                    }

            results = await asyncio.gather(*[process(q) for q in questions])

        return list(results)


def print_results(all_results: list[dict], config: str = "seal-0"):
    if not all_results:
        return

    n = len(all_results)
    correct = sum(1 for r in all_results if r.get("is_correct", False))

    table = Table(title=f"SealQA Results ({config})")
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
    config: str = "seal-0",
    data_dir: str | None = None,
    verbose: bool = True,
) -> list[dict]:
    questions = load_sealqa(config, Path(data_dir) if data_dir else None)

    if not questions:
        console.print("[red]No questions found. Ensure data/sealqa/ has parquet files.[/red]")
        return []

    if limit:
        questions = questions[:limit]

    evaluator = SealQAEvaluator(searcher, rag_agent, grader)
    results = await evaluator.evaluate(
        questions=questions,
        num_results=num_results,
        concurrency=concurrency,
        verbose=verbose,
    )

    if verbose:
        print_results(results, config)

    if output:
        with open(output, "w") as f:
            json.dump(results, f, indent=2)
        console.print(f"\n[green]Results saved to {output}[/green]")

    return results


def main():
    parser = argparse.ArgumentParser(description="SealQA eval")
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
    parser.add_argument("--config", default="seal-0", choices=["seal-0", "seal-hard", "longseal"])
    parser.add_argument("--output", "-o", help="Output file")
    parser.add_argument("--data-dir", help="Path to sealqa data directory")
    args = parser.parse_args()

    if args.info:
        print_info()
        return

    if not args.searcher_api_url:
        console.print("[red]--searcher-api-url is required[/red]")
        return

    if args.mode == "simple":
        from .simple_impl import SimpleSearcher, SimpleRAGAgent
        from .sealqa_simple_grader import SealQASimpleGrader

        searcher = SimpleSearcher(args.searcher_api_url, args.searcher_api_key)
        rag_agent = SimpleRAGAgent(args.rag_model, api_key=args.rag_api_key)
        grader = SealQASimpleGrader(args.grader_model, api_key=args.grader_api_key)
    elif args.mode == "langchain":
        from .langchain_impl import LangChainSearcher, LangChainRAGAgent
        from .sealqa_langchain_grader import SealQALangChainGrader

        searcher = LangChainSearcher(args.searcher_api_url, args.searcher_api_key)
        rag_agent = LangChainRAGAgent(args.rag_model, api_key=args.rag_api_key)
        grader = SealQALangChainGrader(args.grader_model, api_key=args.grader_api_key)

    asyncio.run(run(
        searcher=searcher,
        rag_agent=rag_agent,
        grader=grader,
        limit=args.limit,
        output=args.output,
        num_results=args.num_results,
        concurrency=args.concurrency,
        config=args.config,
        data_dir=args.data_dir,
    ))


if __name__ == "__main__":
    main()
