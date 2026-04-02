from typing import Any, Dict, List, Optional, Union
from OpenAIJsonWrapper import OpenAIJsonWrapper

DEFAULT_QA_TARGET = {
    "score": "int",        # 0-100
    "reason": "string",
}

DEFAULT_QA_CRITERIA = [
    "Consistency: Is the answer consistent with the query?",
    "Relevance: Is the answer relevant to the query?",
    "Clarity: Is the response clear and easy to understand?",
]

DEFAULT_QA_BACKGROUND = ("You are an objective evaluation AI, tasked with assessing the relationship between two text elements.\n"
                         "Always provide a score between 0 and 100, along with a clear reason for the score.")

class DeepEvalQA:
    """Unified evaluation for QA components using separate comparisons.
    
    This class provides a semantic evaluation framework rather than simple JSON wrapping.
    It manages evaluation prompts and ensures responses follow a standardized schema.
    """

    def __init__(
        self,
        client: Any,
        model: str = "gpt-4",
        target_structure: Optional[Dict[str, Any]] = None,
        criteria: Optional[Union[str, List[str]]] = None,
        background: Optional[str] = None,
    ):
        if OpenAIJsonWrapper is None:
            raise ImportError("OpenAIJsonWrapper not available. Please ensure the package is importable.")

        self.client = client
        self.model = model
        self.target_structure = target_structure or DEFAULT_QA_TARGET
        self.criteria = criteria or DEFAULT_QA_CRITERIA
        self.background = background or DEFAULT_QA_BACKGROUND
        
        self._wrapper = OpenAIJsonWrapper(
            client=client,
            model=model,
            target_structure=self.target_structure,
            background=self.background,
            requirements=self.criteria
        )

    def _prepare_messages(self, query: str, answer: str) -> List[Dict[str, str]]:
        """Constructs a semantically rich evaluation prompt."""
        body = (
            f"### QUERY:\n{query}\n\n"
            f"### ANSWER:\n{answer}\n\n"
        )
        return [{"role": "user", "content": body}]

    def evaluate(
        self,
        query: str,
        answer: str,
        **kwargs
    ) -> Dict[str, Any]:
        """Performs a single evaluation between two text elements.
        
        :param query: The source text (e.g., user query or ground truth).
        :param answer: The target text (e.g., model output or reference answer).
        :return: Dict containing reasoning, data (parsed), error, etc.
        """
        # 1. Prepare the semantic messages
        messages = self._prepare_messages(query, answer)
        
        # 2. Use the wrapper for structured extraction
        result = self._wrapper.chat(
            messages,
            model=self.model,
            **kwargs
        )
        
        # 3. Semantic normalization (e.g., ensuring score types)
        data = result.get("data")
        if isinstance(data, dict) and "score" in data:
            try:
                data["score"] = int(data["score"])
            except (ValueError, TypeError):
                pass
            
        return result
