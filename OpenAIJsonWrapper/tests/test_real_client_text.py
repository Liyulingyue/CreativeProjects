import os
import sys
import json

# 让脚本可直接 `python tests/test_real_client_text.py` 运行：
# 当 editable install 没有把仓库里的 OpenAIJsonWrapper/ 目录暴露为
# `openaijsonwrapper` 这个可导入名时，把包目录加进 sys.path。
_PKG_DIR = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", "openaijsonwrapper"))
if _PKG_DIR not in sys.path:
    sys.path.insert(0, _PKG_DIR)
try:
    from openaijsonwrapper import OpenAIJsonWrapper
except ImportError:
    from openaijsonwrapper import OpenAIJsonWrapper  # type: ignore

from openai import OpenAI

def test_real_openai_wrapper():
    # 优先从环境变量获取配置，参考 OpenAIProxy.py 的默认值
    api_key = os.getenv("OPENAI_API_KEY", "your-api-key-here")
    base_url = os.getenv("OPENAI_BASE_URL", "https://aistudio.baidu.com/llm/lmapi/v3")
    model_name = os.getenv("OPENAI_MODEL_NAME", "ernie-4.5-21b-a3b")  # 可根据实际情况修改
    
    if api_key == "your-api-key-here":
        print("警告: 未检测到 OPENAI_API_KEY 环境变量，请确保已设置或手动修改脚本逻辑。")
    
    # 1. 初始化真实的 OpenAI 客户端
    client = OpenAI(api_key=api_key, base_url=base_url)
    
    # 2. 定义期望的结构
    target_structure = {
        "analysis": {
            "sentiment": "string (Positive/Negative/Neutral)",
            "key_entities": ["string"],
            "confidence_score": "float (0-1)"
        },
        "response_suggestion": "string"
    }

    # 3. 封装 Wrapper (将 model 和 structure 放在 init 传入)
    wrapper = OpenAIJsonWrapper(
        client, 
        model=model_name,
        target_structure=target_structure
    )
    
    # 4. 模拟请求
    messages = [
        {"role": "user", "content": "GitHub Copilot 是一款非常棒的 AI 编程助手，它能极大地提高代码生产力，虽然偶尔会有小瑕疵，但整体瑕不掩瑜。"}
    ]
    
    requirements = [
        "情感分析必须细化到分值",
        "key_entities 至少提取两个",
        "response_suggestion 必须是中文"
    ]
    
    background = "你是一个专业级的产品评论分析专家，擅长从用户反馈中提取结构化洞察。"
    
    print("--- 正在发送请求到 LLM ---")
    
    # 测试新增的 requirements 和 background 参数
    result = wrapper.chat(
        messages=messages,
        requirements=requirements,
        background=background
    )
    
    print("\n--- 解析结果 ---")
    if not result["error"]:
        print("成功解析数据:")
        print(json.dumps(result["data"], indent=2, ensure_ascii=False))
        print("\n思维链/推理过程:")
        print(result["reasoning"])
    else:
        print("解析失败!")
        print(f"错误信息: {result['error']}")
        print(f"原始响应内容:\n{result['raw_content']}")

if __name__ == "__main__":
    test_real_openai_wrapper()
