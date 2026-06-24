import os
import sys
import json
import dotenv

dotenv.load_dotenv()

_PKG_DIR = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", "OpenAIJsonWrapper"))
if _PKG_DIR not in sys.path:
    sys.path.insert(0, _PKG_DIR)
try:
    from openaijsonwrapper import OpenAIJsonWrapper
except ImportError:
    from OpenAIJsonWrapper import OpenAIJsonWrapper

from openai import OpenAI

apiKey = os.getenv("OPENAI_API_KEY", "your-api-key-here")
baseUrl = os.getenv("OPENAI_BASE_URL", "https://api.minimaxi.com/v1")
modelName = os.getenv("OPENAI_VISION_MODEL_NAME", "MiniMax-M3")

imagePath = os.getenv("TEST_IMAGE_PATH", "path/to/image.jpg")
imageUrl = os.getenv(
    "TEST_IMAGE_URL",
    "https://upload.wikimedia.org/wikipedia/commons/thumb/3/3a/Cat03.jpg/1200px-Cat03.jpg",
)

def testChatWithImagePath():
    if not imagePath or not os.path.exists(imagePath):
        print(f"跳过: 未设置 TEST_IMAGE_PATH 或文件不存在: {imagePath}")
        return

    if apiKey == "your-api-key-here":
        print("警告: 未检测到 OPENAI_API_KEY 环境变量，请确保已设置或手动修改脚本逻辑。")

    client = OpenAI(api_key=apiKey, base_url=baseUrl)

    targetStructure = {
        "score": "int, 0-100, 代表照片质量评分",
        "style": "str, 照片风格描述",
        "caption": "str, 用中文写一句话，不超过 30 字",
        "main_objects": "list[str], 至少 2 个主要物体",
        "blurry": "str, 照片是否模糊，'模糊'、'略微模糊'、'清晰' 三选一",
        "comments": "str, 对照片的详细评价，至少 50 字",
        "recommendations": "str, 对拍摄者的改进建议，至少 30 字",
    }

    wrapper = OpenAIJsonWrapper(
        client,
        model=modelName,
        target_structure=targetStructure,
        background="你是一名专业的旅行照片分析师，擅长从图片中分析出丰富的细节和信息。",
        requirements=[
            "照片的评价评分需要基于照片的清晰度、构图、色彩和主题等因素综合评定。",
            "请确保输出的 JSON 严格符合指定的结构和类型要求。",
        ]
    )

    messages = [
        {
            "role": "user",
            "content": [
                {"type": "text", "text": "请仔细观察这张图片，按指定 JSON 结构输出。"},
                {"type": "image_path", "image_path": imagePath},
            ],
        }
    ]

    print("--- [image_path] 正在发送多模态请求 ---")

    result = wrapper.chat(messages=messages)

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
    testChatWithImagePath()
