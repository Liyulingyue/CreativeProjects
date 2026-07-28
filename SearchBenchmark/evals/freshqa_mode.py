"""FreshQA eval — 支持多种模式.

Modes:
    simple: 直接 OpenAI SDK，简化 prompt
    langchain: LangChain 版本，对齐 Tavily/web-search-api-evals

Eval modes:
    relaxed: 宽松模式
    strict: 严格模式
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

FRESHQA_GRADING_SYSTEM_RELAXED = """\
You are evaluating the factuality of a model's answer to a question.
The question may have a false premise that the model should reject.
Evaluate whether the answer is factually correct based on the ground truth answers provided.

Mode: RELAXED
- A answer is correct if it contains the correct information
- Minor paraphrasing is acceptable
- If the question has a false premise, saying "I cannot answer" is acceptable
"""

FRESHQA_GRADING_SYSTEM_STRICT = """\
You are evaluating the factuality of a model's answer to a question.
The question may have a false premise that the model should reject.
Evaluate whether the answer is factually correct based on the ground truth answers provided.

Mode: STRICT
- A answer is correct only if it is precise and complete
- Partial information is not acceptable
- If the question has a false premise, saying "I cannot answer" is NOT acceptable
"""

FRESHQA_GRADING_USER = """\
Question: {question}
Ground Truth Answers: {answers}
Model's Answer: {predicted}

Is the model's answer correct? Answer YES or NO."""


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
        answers: list[str],
        predicted_answer: str,
        mode: str = "relaxed",
    ) -> dict[str, Any]:
        raise NotImplementedError


def load_freshqa(csv_path: Path | None = None) -> list[dict]:
    if csv_path is None:
        csv_path = DEFAULT_DATA_DIR / "freshqa" / "FreshQA_v112425 - freshqa.csv"

    if not csv_path.exists():
        console.print(f"[yellow]File not found: {csv_path}[/yellow]")
        return []

    questions = []
    with open(csv_path, newline="", encoding="utf-8") as f:
        reader = csv.reader(f)
        next(reader)
        next(reader)
        header = next(reader)
        for row in reader:
            if len(row) < 10:
                continue
            row_dict = dict(zip(header, row))
            question = row_dict.get("question", "")
            if not question:
                continue
            answers = []
            for i in range(10):
                key = f"answer_{i}"
                if row_dict.get(key):
                    answers.append(row_dict[key])
            if not answers:
                continue
            questions.append({
                "id": row_dict.get("id", ""),
                "split": row_dict.get("split", ""),
                "question": question,
                "answers": answers,
                "effective_year": row_dict.get("effective_year", ""),
                "false_premise": row_dict.get("false_premise", ""),
                "num_hops": row_dict.get("num_hops", ""),
                "fact_type": row_dict.get("fact_type", ""),
            })
    return questions


def print_info():
    questions = load_freshqa()
    console.print("\n[bold]FreshQA Dataset[/bold]")
    console.print("  Factuality evaluation with search engine augmentation.")
    console.print("  Questions may have false premises.\n")

    if questions:
        console.print(f"  Total questions: {len(questions)}")
        splits = set(q.get("split", "") for q in questions)
        console.print(f"  Splits: {splits}")
        console.print(f"  Sample question: {questions[0].get('question', '')[:80]}...")
        console.print(f"  Sample answers: {questions[0].get('answers', [])[:2]}")
    else:
        console.print("  [yellow]No questions loaded.[/yellow]")
    console.print()


class FreshQAEvaluator:
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
        eval_mode: str = "relaxed",
        verbose: bool = True,
    ) -> list[dict]:
        semaphore = asyncio.Semaphore(concurrency)
        results = []

        if verbose:
            console.print(f"[bold]Evaluating {len(questions)} FreshQA questions[/bold]")
            console.print(f"  Searcher: {self.searcher.name}")
            console.print(f"  RAG Model: {self.rag_agent.model}")
            console.print(f"  Grader Model: {self.grader.model}")
            console.print(f"  Eval Mode: {eval_mode}\n")

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
                    question_text = q.get("question", "")
                    answers = q.get("answers", [])
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

                    try:
                        rag_result = await self.rag_agent.synthesize(question_text, search_results)
                        predicted = rag_result.get("answer", "")
                    except Exception as e:
                        logger.warning(f"RAG synthesis failed: {e}")
                        predicted = "I don't know"

                    try:
                        grade = await self.grader.grade(
                            question=question_text,
                            answers=answers,
                            predicted_answer=predicted,
                            mode=eval_mode,
                        )
                        is_correct = grade.get("is_correct", False)
                    except Exception as e:
                        logger.warning(f"Grading failed: {e}")
                        is_correct = False

                    progress.advance(task_id)

                    return {
                        "id": question_id,
                        "query": question_text,
                        "answers": answers,
                        "predicted_answer": predicted,
                        "is_correct": is_correct,
                        "latency": round(latency, 2),
                    }

            results = await asyncio.gather(*[process(q) for q in questions])

        return list(results)


def print_results(all_results: list[dict], eval_mode: str = "relaxed"):
    if not all_results:
        return

    n = len(all_results)
    correct = sum(1 for r in all_results if r.get("is_correct", False))

    table = Table(title=f"FreshQA Results ({eval_mode.capitalize()} Mode)")
    table.add_column("Metric", style="cyan")
    table.add_column("Value", justify="right")

    table.add_row("Total Questions", str(n))
    table.add_row("Correct", str(correct))
    table.add_row("Accuracy", f"{correct / n:.1%}")

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
    eval_mode: str = "relaxed",
    csv_path: str | None = None,
    verbose: bool = True,
) -> list[dict]:
    questions = load_freshqa(Path(csv_path) if csv_path else None)

    if not questions:
        console.print("[red]No questions found.[/red]")
        return []

    if limit:
        questions = questions[:limit]

    evaluator = FreshQAEvaluator(searcher, rag_agent, grader)
    results = await evaluator.evaluate(
        questions=questions,
        num_results=num_results,
        concurrency=concurrency,
        eval_mode=eval_mode,
        verbose=verbose,
    )

    if verbose:
        print_results(results, eval_mode)

    if output:
        with open(output, "w") as f:
            json.dump(results, f, indent=2)
        console.print(f"\n[green]Results saved to {output}[/green]")

    return results


def main():
    parser = argparse.ArgumentParser(description="FreshQA eval")
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
    parser.add_argument("--eval-mode", default="relaxed", choices=["relaxed", "strict"])
    parser.add_argument("--output", "-o", help="Output file")
    parser.add_argument("--csv-path", help="Path to FreshQA CSV")
    args = parser.parse_args()

    if args.info:
        print_info()
        return

    if not args.searcher_api_url:
        console.print("[red]--searcher-api-url is required[/red]")
        return

    if args.mode == "simple":
        from .simple_impl import SimpleSearcher, SimpleRAGAgent
        from .freshqa_simple_grader import FreshQASimpleGrader

        searcher = SimpleSearcher(args.searcher_api_url, args.searcher_api_key)
        rag_agent = SimpleRAGAgent(args.rag_model, api_key=args.rag_api_key)
        grader = FreshQASimpleGrader(args.grader_model, api_key=args.grader_api_key)
    elif args.mode == "langchain":
        from .langchain_impl import LangChainSearcher, LangChainRAGAgent
        from .freshqa_langchain_grader import FreshQALangChainGrader

        searcher = LangChainSearcher(args.searcher_api_url, args.searcher_api_key)
        rag_agent = LangChainRAGAgent(args.rag_model, api_key=args.rag_api_key)
        grader = FreshQALangChainGrader(args.grader_model, api_key=args.grader_api_key)

    asyncio.run(run(
        searcher=searcher,
        rag_agent=rag_agent,
        grader=grader,
        limit=args.limit,
        output=args.output,
        num_results=args.num_results,
        concurrency=args.concurrency,
        eval_mode=args.eval_mode,
        csv_path=args.csv_path,
    ))


if __name__ == "__main__":
    main()
