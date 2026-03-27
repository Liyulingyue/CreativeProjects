from dotenv import load_dotenv
import os

from deepeval.metrics import FaithfulnessMetric
from deepeval.test_case import LLMTestCase
from deepeval.models import GPTModel

# 加载环境变量配置文件
# 调用机制：load_dotenv() 会读取当前目录下的 .env 文件，并将变量加载到环境变量中
load_dotenv()

# 从环境变量读取配置
# 调用机制：os.getenv() 用于获取环境变量，如果不存在则使用默认值
model_name = os.getenv("LITELLM_MODEL", "ernie-4.5-21b-a3b")
api_key = os.getenv("LITELLM_API_KEY")
base_url = os.getenv("LITELLM_API_BASE")

if not api_key or not base_url:
    raise RuntimeError("Missing API Key or Base URL in environment.")

# 使用 DeepEval 中的 GPTModel（它其实就是 OpenAI API 的包装器）
# 调用机制：GPTModel 类封装了 OpenAI API 调用，支持自定义 base_url 以对接不同服务
# 它会使用提供的 api_key 和 base_url 来初始化模型实例
custom_model = GPTModel(
    model=model_name,
    api_key=api_key,
    base_url=base_url
)

# 创建 FaithfulnessMetric 实例
# 调用机制：FaithfulnessMetric 使用指定的模型来评估输出是否忠实于上下文
# threshold=0.7 表示评估阈值，model=custom_model 指定使用的评估模型
metric = FaithfulnessMetric(threshold=0.7, model=custom_model)

# 创建测试案例
# 调用机制：LLMTestCase 封装了输入、实际输出和检索上下文
# 注意点：对于 FaithfulnessMetric（忠实度指标）来说，'input' 并不是计算核心得分的必要参数。
# 该指标主要关注 'actual_output'（回答）是否完全来源于 'retrieval_context'（检索到的上下文）。
# 但定义 'input' 对于日志记录、报告生成以及后续扩展其他指标（如相关性）非常重要。
test_case = LLMTestCase(
    input="1+1等于多少？", 
    actual_output="2", 
    retrieval_context=["1+1的结果是2。"]
)

# 执行评估测量
# 调用机制：metric.measure(test_case) 不仅仅是简单的对比，它的内部流程如下：
# 1. 提取：调用模型从 actual_output 中提取出多个独立的事实点（Claims）。
# 2. 验证：逐一调用模型检查这些事实点是否都能在 retrieval_context 中找到支持。
# 3. 计算：根据通过验证的事实点比例计算最终的 Faithfulness 分数。
metric.measure(test_case)

# 输出评估结果
# 调用机制：metric.score 是计算出的分数（0-1之间），metric.reason 是评估的详细解释
print(f"Score: {metric.score}")
print(f"Reason: {metric.reason}")