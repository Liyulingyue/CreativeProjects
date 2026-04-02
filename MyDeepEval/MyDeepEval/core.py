import json
from typing import Any, Dict, List, Optional, Union
from OpenAIJsonWrapper import OpenAIJsonWrapper


DEFAULT_TARGET = {
    "score": "int",        # 0-100
    "reason": "string",
}


DEFAULT_CRITERIA = [
    "Correctness: Does the response contain factual errors?",
    "Completeness: Does the response cover all aspects of the question?",
    "Clarity: Is the response clear and easy to understand?",
    "Conciseness: Is the response free of unnecessary information?",
]

DEFAULT_BACKGROUND = ("You are an objective evaluation AI, tasked with assessing the quality of model responses based on provided criteria.\n"
                      "Always provide a score between 0 and 100, along with a clear reason for the score.")


class DeepEval:
    """Semi-intelligent evaluation helper built on top of OpenAIJsonWrapper.
    
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
        self.target_structure = target_structure or DEFAULT_TARGET
        self.criteria = criteria or DEFAULT_CRITERIA
        self.background = background or DEFAULT_BACKGROUND
        
        # Internal wrapper used for actual structured generation
        self._wrapper = OpenAIJsonWrapper(
            client=client, 
            model=model, 
            target_structure=self.target_structure,
            background=self.background,
            requirements=self.criteria
        )

    def _prepare_eval_messages(self, model_output: str, reference_output: str) -> List[Dict[str, str]]:
        """Constructs a semantically rich evaluation prompt."""
        eval_body = (
            f"### MODEL OUTPUT TO EVALUATE:\n{model_output}\n\n"
            f"### REFERENCE OUTPUT (GROUND TRUTH):\n{reference_output}\n\n"
        )
        
        return [{"role": "user", "content": eval_body}]

    def evaluate(
        self,
        model_output: str,
        reference_output: str,
        **kwargs,
    ) -> Dict[str, Any]:
        """Performs a semantic evaluation of the model output against a reference.
        
        :param model_output: The actual output from the model to be evaluated.
        :param reference_output: The expected or reference output (ground truth).
        :return: Dict containing reasoning, data (parsed), error, etc.
        """
        # 1. Prepare the semantic messages
        messages = self._prepare_eval_messages(model_output, reference_output)

        # 2. Use the wrapper for structured extraction
        # Fixed to use the target structure and criteria set during initialization
        result = self._wrapper.chat(
            messages,
            model=self.model,
            **kwargs
        )

        # 4. Semantic normalization (e.g., ensuring score types)
        data = result.get("data")
        if isinstance(data, dict) and "score" in data:
            try:
                data["score"] = int(data["score"])
            except (ValueError, TypeError):
                pass
                
        return result
