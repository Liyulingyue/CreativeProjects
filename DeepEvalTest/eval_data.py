from dotenv import load_dotenv
import os
from deepeval import evaluate
from deepeval.metrics import FaithfulnessMetric
from deepeval.test_case import LLMTestCase
from deepeval.models import GPTModel
from deepeval.evaluate import AsyncConfig

# 加载 .env 配置
load_dotenv()

# 配置自定义模型
model_name = os.getenv("LITELLM_MODEL", "ernie-4.0-8k-latest")
api_key = os.getenv("LITELLM_API_KEY")
base_url = os.getenv("LITELLM_API_BASE")

custom_model = GPTModel(
    model=model_name,
    api_key=api_key,
    base_url=base_url
)

# 1. 定义数据集 (把你的数据转为测试用例列表)
test_cases = [
    LLMTestCase(
        input="周杰伦是哪年出生的？",
        actual_output="周杰伦出生于 1979 年。",
        retrieval_context=["周杰伦（Jay Chou），1979年1月18日出生于台湾。"]
    ),
    LLMTestCase(
        input="1+1等于多少？", 
        actual_output="1+1=2", 
        retrieval_context=["算术规则中，1+1的结果是2。"]
    )
    # ... 在这里添加更多数据
]

# 2. 初始化指标 (使用自定义模型)
metric = FaithfulnessMetric(
    threshold=0.7, 
    model=custom_model,
)

# 3. 一键运行评测数据集
# 运行后会显示详细的进度条和结果表格
if __name__ == "__main__":
    # 处理 API 速率限制的配置
    async_config = AsyncConfig(
        run_async=True,
        throttle_value=1,  # 每个测试用例间隔 1 秒
        max_concurrent=1   # 限制并发数为 1，确保串行执行
    )

    evaluate(
        test_cases, 
        [metric],
        async_config=async_config
    )