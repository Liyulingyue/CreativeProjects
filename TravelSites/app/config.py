import os
from pathlib import Path
from dotenv import load_dotenv

_ENV_PATH = Path(__file__).resolve().parent.parent / ".env"
load_dotenv(_ENV_PATH)

API_KEY: str = os.getenv("OPENAI_API_KEY", "your-api-key-here")
BASE_URL: str = os.getenv("OPENAI_BASE_URL", "https://api.minimaxi.com/v1")
MODEL_NAME: str = os.getenv("OPENAI_VISION_MODEL_NAME", "MiniMax-M3")

REFRESH_ENABLED: bool = os.getenv("REFRESH_ENABLED", "false").lower() in ("true", "1", "yes")
REFRESH_INTERVAL_SECONDS: int = int(os.getenv("REFRESH_INTERVAL_SECONDS", "3600"))

SEED_CITIES: list[str] = [
    "济南", "大同", "青岛", "烟台", "威海",
    "杭州", "苏州", "南京", "宁波", "绍兴",
    "厦门", "福州", "泉州", "霞浦",
    "西安", "成都", "重庆", "昆明", "大理", "丽江",
    "桂林", "北海", "涠洲岛",
    "三亚", "海口", "万宁",
    "黄山", "宏村", "婺源", "千岛湖",
    "敦煌", "张掖", "嘉峪关",
    "拉萨", "林芝",
]

MATRIX_MAX_OFFSET: int = int(os.getenv("MATRIX_MAX_OFFSET", "1"))
MATRIX_MAX_DURATION: int = int(os.getenv("MATRIX_MAX_DURATION", "2"))
MATRIX_CONCURRENCY: int = int(os.getenv("MATRIX_CONCURRENCY", "3"))

DATA_DIR: Path = Path(__file__).resolve().parent.parent / "data"
MATRIX_CACHE_DIR: Path = DATA_DIR / "matrix_cache"
MATRIX_CACHE_DIR.mkdir(parents=True, exist_ok=True)