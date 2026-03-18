import os
from ragas import EvaluationDataset, evaluate
from ragas.metrics import Faithfulness, AnswerRelevancy
from openai import OpenAI
from ragas.llms import llm_factory
from ragas.embeddings import LangchainEmbeddingsWrapper
from langchain_openai import OpenAIEmbeddings as LangchainOpenAIEmbeddings

# 1. 准备你的测试数据 (Ragas v0.2+ 使用新的标准列名)
test_data = [
    {
        "user_input": "周杰伦是哪年出生的？",
        "response": "周杰伦出生于 1979 年。",
        "retrieved_contexts": ["周杰伦（Jay Chou），1979年1月18日出生于台湾省新北市。"],
        "reference": "1979年" 
    }
]

# 2. 将数据转换为 Ragas 可识别的格式
dataset = EvaluationDataset.from_list(test_data)

# 配置你的自定义模型参数
MODEL_NAME = "ernie-4.5-21b-a3b"
MODEL_KEY = "**********"  # 替换为你的模型 API Key
MODEL_URL = "https://aistudio.baidu.com/llm/lmapi/v3"

EMBEDDING_NAME = "bge-large-zh"
EMBEDDING_KEY = "***********"  # 替换为你的嵌入模型 API Key
EMBEDDING_URL = "https://aistudio.baidu.com/llm/lmapi/v3"

# 3. 配置自定义模型 (Ragas v0.4+ 推荐方式)
client = OpenAI(api_key=MODEL_KEY, base_url=MODEL_URL)
custom_llm = llm_factory(model=MODEL_NAME, client=client)

# 使用 Langchain 的 OpenAIEmbeddings 并通过 LangchainEmbeddingsWrapper 包装
# 这能确保提供 Ragas 指标所需的 embed_query 方法
raw_embeddings = LangchainOpenAIEmbeddings(
    model=EMBEDDING_NAME, 
    openai_api_key=EMBEDDING_KEY, 
    openai_api_base=EMBEDDING_URL
)
custom_embeddings = LangchainEmbeddingsWrapper(raw_embeddings)

# 4. 运行评估
f = Faithfulness(llm=custom_llm)
ar = AnswerRelevancy(llm=custom_llm, embeddings=custom_embeddings)

# 百度 AI Studio 暂不支持 n > 1 的请求 (n=3 is default in Ragas)，将其设为 1
ar.n = 1 

results = evaluate(
    dataset=dataset,
    metrics=[f, ar],
)

print("\n--- 评估结果 ---")
print(results)
print(results.to_pandas())

# 5. 输出结果
print("\n--- 评估结果 ---")
print(results)

df = results.to_pandas()
print(df[['user_input', 'faithfulness', 'answer_relevancy']])
