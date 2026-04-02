from MyDeepEval import DeepEvalQA
from openai import OpenAI
import json

# 配置 (复用原有的测试配置)
MODEL_URL = "https://api-inference.modelscope.cn/v1"
MODEL_KEY = "ms-***************"
MODEL_NAME = "Qwen/Qwen3-30B-A3B-Instruct-2507"

def run_qa_evaluation_demo():
    """演示如何使用 DeepEvalQA 进行多维度评估"""
    client = OpenAI(
        api_key=MODEL_KEY,
        base_url=MODEL_URL
    )
    
    # 初始化原子评估器
    # 使用统一的参数命名：target_structure 和 criteria
    de_qa = DeepEvalQA(
        client, 
        model=MODEL_NAME,
        criteria=[
            "契合度：问题与回复是否在语义上紧密相关？",
        ]
    )
    
    # 测试数据
    query = "如何有效地学习 Python？"
    answer = "建议多刷 LeetCode，并阅读《流畅的 Python》。"   

    print(f"--- 正在使用 {MODEL_NAME} 进行 QA 维度评估 ---")
    res = de_qa.evaluate(query, answer)
    
    if res["error"]:
        print(f"[错误] 评估失败: {res['error']}")
        return

    print(f"[成功] 思维链: {res['reasoning']}")
    print(f"[成功] 评分: {res['data']['score']}")
    print(f"[成功] 理由: {res['data']['reason']}")
    
    assert res["data"] is not None
    assert isinstance(res["data"]["score"], int)
    assert 0 <= res["data"]["score"] <= 100

if __name__ == "__main__":
    # 如果直接运行脚本，执行真实 API 测试
    run_qa_evaluation_demo()