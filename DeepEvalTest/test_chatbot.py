from dotenv import load_dotenv
import os
import pytest
from deepeval import assert_test, evaluate
from deepeval.metrics import GEval
from deepeval.test_case import LLMTestCase, LLMTestCaseParams
from deepeval.models import GPTModel
from deepeval.evaluate import AsyncConfig

# 加载 .env
load_dotenv()

# 从环境变量读取配置
model_name = os.getenv("LITELLM_MODEL", "ernie-4.0-8k-latest")
api_key = os.getenv("LITELLM_API_KEY")
base_url = os.getenv("LITELLM_API_BASE")

if not api_key or not base_url:
    raise RuntimeError("Missing API Key or Base URL in environment.")

# 使用 DeepEval 中的 GPTModel
custom_model = GPTModel(
    model=model_name,
    api_key=api_key,
    base_url=base_url
)

def test_case():
    correctness_metric = GEval(
        name="Correctness",
        criteria="Determine if the 'actual output' is correct based on the 'expected output'.",
        evaluation_params=[LLMTestCaseParams.ACTUAL_OUTPUT, LLMTestCaseParams.EXPECTED_OUTPUT],
        threshold=0.5,
        model=custom_model # 将自定义模型传给指标
    )
    test_case = LLMTestCase(
        input="What if these shoes don't fit?",
        # Replace this with the actual output from your LLM application
        actual_output="You have 30 days to get a full refund at no extra cost.",
        expected_output="We offer a 30-day full refund at no extra costs.",
        retrieval_context=["All customers are eligible for a 30 day full refund at no extra costs."]
    )
    assert_test(test_case, [correctness_metric])
    
# 运行测试
# deepeval test run test_chatbot.py