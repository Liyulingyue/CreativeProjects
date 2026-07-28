from dataclasses import dataclass
from typing import Any


@dataclass
class SimpleQAMetrics:
    is_correct: float = 0.0
    is_incorrect: float = 0.0
    is_not_attempted: float = 0.0
    accuracy_given_attempted: float = 0.0
    f1: float = 0.0
    num_samples: int = 0


def compute_simpleqa_metrics(grades: list[dict[str, Any]]) -> SimpleQAMetrics:
    if not grades:
        return SimpleQAMetrics()

    n = len(grades)
    is_correct = sum(1 for g in grades if g.get("grade_letter") == "A")
    is_incorrect = sum(1 for g in grades if g.get("grade_letter") == "B")
    is_not_attempted = sum(1 for g in grades if g.get("grade_letter") == "C")

    accuracy_given_attempted = (
        is_correct / (is_correct + is_incorrect)
        if (is_correct + is_incorrect) > 0
        else 0.0
    )

    f1 = (
        2 * accuracy_given_attempted * is_correct / (accuracy_given_attempted + is_correct)
        if (accuracy_given_attempted + is_correct) > 0
        else 0.0
    )

    return SimpleQAMetrics(
        is_correct=is_correct / n,
        is_incorrect=is_incorrect / n,
        is_not_attempted=is_not_attempted / n,
        accuracy_given_attempted=accuracy_given_attempted,
        f1=f1,
        num_samples=n,
    )
