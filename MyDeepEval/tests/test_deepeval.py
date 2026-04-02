from MyDeepEval import DeepEval
from openai import OpenAI

# MODEL_URL="https://api-inference.modelscope.cn/v1"
# MODEL_KEY="**************************"
# MODEL_NAME="Qwen/Qwen3-30B-A3B-Instruct-2507"
MODEL_URL="https://qianfan.baidubce.com/v2/coding"
MODEL_KEY="***************************"
MODEL_NAME="minimax-m2.5"

def test_deepeval_real_api():
    """使用真实 API 进行集成测试"""
    client = OpenAI(
        api_key=MODEL_KEY,
        base_url=MODEL_URL
    )
    
    # 初始化 DeepEval，设定标准
    de = DeepEval(
        client, 
        model=MODEL_NAME,
        criteria=[
            "准确性：模型回复是否与参考答案在核心事实上保持一致？",
            "如果回复冗余不作扣分"
        ]
    )
    
    model_output = "北京是中国的首都，是一个现代化的国际大都市。"
    reference_output = "中国的首都是北京。"
    
    print(f"\n[测试] 正在使用模型 {MODEL_NAME} 进行真实集成测试...")
    res = de.evaluate(model_output, reference_output)
    
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
    test_deepeval_real_api()