import os
from pathlib import Path
from typing import Optional
from dotenv import load_dotenv

load_dotenv()

API_KEY: str = os.getenv("OPENAI_API_KEY", "your-api-key-here")
BASE_URL: str = os.getenv("OPENAI_BASE_URL", "https://api.minimaxi.com/v1")
MODEL_NAME: str = os.getenv("OPENAI_VISION_MODEL_NAME", "MiniMax-M3")

SUPPORTED_IMAGE_FORMATS: set = {".jpg", ".jpeg", ".png", ".gif", ".bmp", ".webp", ".tiff"}

DEFAULT_TARGET_STRUCTURE: dict = {
    "score": "int, 0-100, 代表照片质量评分",
    "style": "str, 照片风格描述",
    "caption": "str, 用中文写一句话，不超过 30 字",
    "main_objects": "list[str], 至少 2 个主要物体",
    "blurry": "str, 照片是否模糊，'模糊'、'略微模糊'、'清晰' 三选一",
    "comments": "str, 对照片的详细评价，至少 50 字",
    "recommendations": "str, 对拍摄者的改进建议，至少 30 字",
}

DEFAULT_BACKGROUND: str = "你是一名专业的旅行照片分析师，擅长从图片中分析出丰富的细节和信息。"

DEFAULT_REQUIREMENTS: list = [
    "照片的评价评分需要基于照片的清晰度、构图、色彩和主题等因素综合评定。",
    "请确保输出的 JSON 严格符合指定的结构和类型要求。",
]


def is_image_file(file_path: str | Path) -> bool:
    return Path(file_path).suffix.lower() in SUPPORTED_IMAGE_FORMATS


def get_image_files(path: str | Path) -> list[Path]:
    path = Path(path)
    if path.is_file():
        return [path] if is_image_file(path) else []
    return [f for f in path.rglob("*") if f.is_file() and is_image_file(f)]


def ensure_dir(dir_path: str | Path) -> Path:
    path = Path(dir_path)
    path.mkdir(parents=True, exist_ok=True)
    return path
