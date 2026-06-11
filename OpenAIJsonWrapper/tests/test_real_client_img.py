import os
import sys
import json

# 让脚本可直接 `python tests/test_real_client_img.py` 运行：
# 当 editable install 没有把仓库里的 OpenAIJsonWrapper/ 目录暴露为
# `openaijsonwrapper` 这个可导入名时，把包目录加进 sys.path。
_PKG_DIR = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", "OpenAIJsonWrapper"))
if _PKG_DIR not in sys.path:
    sys.path.insert(0, _PKG_DIR)
try:
    from openaijsonwrapper import OpenAIJsonWrapper
except ImportError:
    from OpenAIJsonWrapper import OpenAIJsonWrapper  # type: ignore

from openai import OpenAI

# 优先从环境变量获取配置
api_key = os.getenv("OPENAI_API_KEY", "your-api-key-here")
base_url = os.getenv("OPENAI_BASE_URL", "https://api.minimaxi.com/v1")
model_name = os.getenv("OPENAI_VISION_MODEL_NAME", "MiniMax-M3")

image_path = os.getenv("TEST_IMAGE_PATH", "demo.jpg")
image_url = os.getenv(
    "TEST_IMAGE_URL",
    "https://upload.wikimedia.org/wikipedia/commons/thumb/3/3a/Cat03.jpg/1200px-Cat03.jpg",
)

def test_chat_with_image_path():
    """测试 chat() 中通过 image_path part 传入本地图片。"""

    if not image_path or not os.path.exists(image_path):
        print(f"跳过: 未设置 TEST_IMAGE_PATH 或文件不存在: {image_path}")
        return

    if api_key == "your-api-key-here":
        print("警告: 未检测到 OPENAI_API_KEY 环境变量，请确保已设置或手动修改脚本逻辑。")

    # 1. 初始化真实的 OpenAI 客户端
    client = OpenAI(api_key=api_key, base_url=base_url)

    # 2. 定义期望的结构
    target_structure = {
        "caption": "string (图片的中文描述)",
        "main_objects": ["string (图中识别到的主要物体)"],
        "color_palette": ["string (主要颜色，hex 或描述皆可)"],
    }

    # 3. 封装 Wrapper
    wrapper = OpenAIJsonWrapper(
        client,
        model=model_name,
        target_structure=target_structure,
    )

    # 4. 构造多模态 messages
    messages = [
        {
            "role": "user",
            "content": [
                {"type": "text", "text": "请仔细观察这张图片，按指定 JSON 结构输出。"},
                {"type": "image_path", "image_path": image_path},
            ],
        }
    ]

    requirements = [
        "caption 用中文写一句话，不超过 30 字",
        "main_objects 至少 2 个",
        "color_palette 至少 3 个颜色",
    ]
    background = "你是一名专业的图像内容分析助手，擅长抽取结构化视觉信息。"

    print("--- [image_path] 正在发送多模态请求 ---")

    result = wrapper.chat(
        messages=messages,
        requirements=requirements,
        background=background,
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


def test_chat_with_image_url():
    """测试 chat() 中通过 image_url part 传入远程 URL。"""

    if api_key == "your-api-key-here":
        print("警告: 未检测到 OPENAI_API_KEY 环境变量，请确保已设置或手动修改脚本逻辑。")

    # 1. 初始化真实的 OpenAI 客户端
    client = OpenAI(api_key=api_key, base_url=base_url)

    # 2. 定义期望的结构
    target_structure = {
        "label": "string (图片分类: cat/dog/other)",
        "confidence": "float (0-1)",
        "reason": "string (一句话中文理由)",
    }

    # 3. 封装 Wrapper
    wrapper = OpenAIJsonWrapper(
        client,
        model=model_name,
        target_structure=target_structure,
    )

    # 4. 构造多模态 messages
    messages = [
        {
            "role": "user",
            "content": [
                {"type": "text", "text": "判断图片主体动物类别。"},
                {"type": "image_url", "image_url": {"url": image_url}},
            ],
        }
    ]

    requirements = [
        "label 只能从 cat/dog/other 中选",
        "confidence 保留两位小数",
        "reason 用中文且不超过 20 字",
    ]

    print("--- [image_url] 正在发送多模态请求 ---")

    result = wrapper.chat(
        messages=messages,
        requirements=requirements,
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
    test_chat_with_image_path()
    print("\n" + "=" * 60 + "\n")
    test_chat_with_image_url()
