from dotenv import load_dotenv
import os
from deepeval.metrics import GEval
from deepeval.test_case import LLMTestCase, LLMTestCaseParams
from deepeval.models import GPTModel
from deepeval.evaluate import evaluate, AsyncConfig

# 1. 加载环境变量配置文件
# 调用机制：load_dotenv() 会读取当前目录下的 .env 文件，并将变量加载到环境变量中
load_dotenv()

# 2. 从环境变量读取配置
# 调用机制：os.getenv() 用于获取环境变量，如果不存在则使用第二个参数作为默认值
model_name = os.getenv("LITELLM_MODEL", "ernie-4.0-8k-latest")
api_key = os.getenv("LITELLM_API_KEY")
base_url = os.getenv("LITELLM_API_BASE")

if not api_key or not base_url:
    raise RuntimeError("Missing API Key or Base URL in environment.")

# 3. 初始化自定义模型 (绕过 OpenAI 限制，对接国内大模型)
# 调用机制：GPTModel 是 DeepEval 对 OpenAI API 的包装器
# 它允许我们通过自定义 base_url 将评估请求发送到国内大模型接口
custom_model = GPTModel(
    model=model_name,
    api_key=api_key,
    base_url=base_url
)

# 4. 定义评估组件
def get_test_suite():
    # 指标定义：使用 G-Eval 框架进行评估
    # 调用机制：G-Eval 是一种基于 LLM 的评估方法。
    # - 它会根据 criteria（标准）驱动 LLM 自动生成一套评分细则（Rubrics）。
    # - 然后由 LLM 根据实际输出与期望输出的对比进行打分。
    # 自定义机制：你可以完全用中文描述 criteria，甚至提供详细的评估步骤来引导模型思维。
    correctness_metric = GEval(
        name="Correctness",
        # 这里是自定义 Prompt 的核心：
        # 你可以将其改为中文，例如："请检查 'actual output' 是否包含了 'expected output' 中的所有关键事实。"
        criteria="Determine if the 'actual output' is correct based on the 'expected output'.",
        # 评估参数：指定模型在评估时需要考虑的字段
        evaluation_params=[LLMTestCaseParams.ACTUAL_OUTPUT, LLMTestCaseParams.EXPECTED_OUTPUT],
        threshold=0.5,
        model=custom_model
    )
    
    # 测试用例定义
    # 调用机制：LLMTestCase 集中管理输入、实际输出、参考上下文和期望输出
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
    # 调用机制：AsyncConfig 用于控制评估任务的并发和速率
    # throttle_value: 控制请求间隔，防止触发 API 频控
    # max_concurrent: 限制同时进行的异步任务数量
    async_config = AsyncConfig(
        run_async=True,
        throttle_value=1,  # 每个测试用例间隔 1 秒
        max_concurrent=1   # 严格限制并发数为 1（串行执行）
    )

    print(f"🚀 开始执行评估，使用模型: {model_name}")
    print(f"🔗 API 端点: {base_url}")
    
    # 执行评估
    # 调用机制：evaluate 函数会遍历 test_cases，为每个用例运行 metrics 中的评估逻辑
    # 并根据 async_config 的配置进行排队执行
    evaluate(
        test_cases, 
        metrics,
        async_config=async_config
    )
