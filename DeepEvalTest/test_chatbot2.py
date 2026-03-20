from dotenv import load_dotenv
import os
from deepeval.metrics import GEval
from deepeval.test_case import LLMTestCase, LLMTestCaseParams
from deepeval.models import GPTModel
from deepeval.evaluate import evaluate, AsyncConfig

# 1. 加载 .env
load_dotenv()

# 2. 从环境变量读取配置
model_name = os.getenv("LITELLM_MODEL", "ernie-4.0-8k-latest")
api_key = os.getenv("LITELLM_API_KEY")
base_url = os.getenv("LITELLM_API_BASE")

if not api_key or not base_url:
    raise RuntimeError("Missing API Key or Base URL in environment.")

# 3. 初始化自定义模型 (绕过 OpenAI 限制)
custom_model = GPTModel(
    model=model_name,
    api_key=api_key,
    base_url=base_url
)

# 4. 定义评估组件
def get_test_suite():
    # 指标定义
    correctness_metric = GEval(
        name="Correctness",
        criteria="Determine if the 'actual output' is correct based on the 'expected output'.",
        evaluation_params=[LLMTestCaseParams.ACTUAL_OUTPUT, LLMTestCaseParams.EXPECTED_OUTPUT],
        threshold=0.5,
        model=custom_model
    )
    
    # 测试用例定义
    test_case = LLMTestCase(
        input="What if these shoes don't fit?",
        actual_output="You have 30 days to get a full refund at no extra cost.",
        expected_output="We offer a 30-day full refund at no extra costs.",
        retrieval_context=["All customers are eligible for a 30 day full refund at no extra costs."]
    )
    
    return [test_case], [correctness_metric]

# 5. 主运行逻辑 (基于 Python 运行且支持速率控制)
if __name__ == "__main__":
    test_cases, metrics = get_test_suite()
    
    # 针对国内 API 的 QPS 限制进行配置
    async_config = AsyncConfig(
        run_async=True,
        throttle_value=1,  # 每个测试用例间隔 1 秒
        max_concurrent=1   # 严格限制并发数为 1（串行执行）
    )

    print(f"🚀 开始执行评估，使用模型: {model_name}")
    print(f"🔗 API 端点: {base_url}")
    
    evaluate(
        test_cases, 
        metrics,
        async_config=async_config
    )
