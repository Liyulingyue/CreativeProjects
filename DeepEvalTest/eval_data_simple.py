from dotenv import load_dotenv
import os

from deepeval.metrics import FaithfulnessMetric
from deepeval.test_case import LLMTestCase
from deepeval.models import GPTModel

# 加载 .env
load_dotenv()

# 从环境变量读取配置
model_name = os.getenv("LITELLM_MODEL", "ernie-4.5-21b-a3b")
api_key = os.getenv("LITELLM_API_KEY")
base_url = os.getenv("LITELLM_API_BASE")

if not api_key or not base_url:
    raise RuntimeError("Missing API Key or Base URL in environment.")

# 使用 DeepEval 中的 GPTModel（它其实就是 OpenAI API 的包装器）
# 它可以接收自定义的 base_url 来对接国内大模型
custom_model = GPTModel(
    model=model_name,
    api_key=api_key,
    base_url=base_url
)

metric = FaithfulnessMetric(threshold=0.7, model=custom_model)
test_case = LLMTestCase(
    input="1+1等于多少？", 
    actual_output="2", 
    retrieval_context=["1+1的结果是2。"]
)

metric.measure(test_case)
print(f"Score: {metric.score}")
print(f"Reason: {metric.reason}")