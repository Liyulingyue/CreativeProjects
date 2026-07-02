import os
from pathlib import Path
from dotenv import load_dotenv
from threading import RLock

_ENV_PATH = Path(__file__).resolve().parent.parent / ".env"
load_dotenv(_ENV_PATH)

API_KEY: str = os.getenv("OPENAI_API_KEY", "your-api-key-here")
BASE_URL: str = os.getenv("OPENAI_BASE_URL", "https://api.minimaxi.com/v1")
MODEL_NAME: str = os.getenv("OPENAI_VISION_MODEL_NAME", "MiniMax-M3")

REFRESH_ENABLED: bool = os.getenv("REFRESH_ENABLED", "false").lower() in ("true", "1", "yes")
REFRESH_INTERVAL_SECONDS: int = int(os.getenv("REFRESH_INTERVAL_SECONDS", "3600"))

# 种子城市列表（fallback，DB 里的 seed_config 优先）
SEED_CITIES: list[str] = [
    c.strip() for c in os.getenv(
        "SEED_CITIES",
        "济南,大同,青岛,烟台,威海,杭州,苏州,南京,宁波,绍兴,"
        "厦门,福州,泉州,霞浦,西安,成都,重庆,昆明,大理,丽江,"
        "桂林,北海,涠洲岛,三亚,海口,万宁,黄山,宏村,婺源,千岛湖,"
        "敦煌,张掖,嘉峪关,拉萨,林芝"
    ).split(",") if c.strip()
]

MATRIX_MAX_OFFSET: int = int(os.getenv("MATRIX_MAX_OFFSET", "1"))
MATRIX_MAX_DURATION: int = int(os.getenv("MATRIX_MAX_DURATION", "2"))
MATRIX_CONCURRENCY: int = int(os.getenv("MATRIX_CONCURRENCY", "3"))

# Admin 账户（启动时自动创建）
ADMIN_USERNAME: str = os.getenv("ADMIN_USERNAME", "admin")
ADMIN_PASSWORD: str = os.getenv("ADMIN_PASSWORD", "admin123")
ADMIN_EMAIL: str = os.getenv("ADMIN_EMAIL", "admin@travelsites.local")
ADMIN_DISPLAY_NAME: str = os.getenv("ADMIN_DISPLAY_NAME", "系统管理员")

# Auth
SESSION_DAYS: int = int(os.getenv("SESSION_DAYS", "30"))

DATA_DIR: Path = Path(__file__).resolve().parent.parent / "data"
MATRIX_CACHE_DIR: Path = DATA_DIR / "matrix_cache"
MATRIX_CACHE_DIR.mkdir(parents=True, exist_ok=True)

# 天气预报缓存
# 启动时自动拉取所有城市的未来 14 天预报
# 拉取的天气数据用于搜索时的天气评分（不需要额外 LLM 调用）
# Open-Meteo 免费 API，0 成本，10,000 次/天额度，这里 374 城 × 1 次 = 374 次/天
WEATHER_FORECAST_DAYS: int = int(os.getenv("WEATHER_FORECAST_DAYS", "14"))
WEATHER_HISTORY_DAYS: int = int(os.getenv("WEATHER_HISTORY_DAYS", "30"))
WEATHER_REFRESH_ON_STARTUP: bool = os.getenv("WEATHER_REFRESH_ON_STARTUP", "true").lower() in ("true", "1", "yes")

# ---------- 运行时配置（可由 admin UI 动态修改） ----------
_config_lock = RLock()
_runtime_config: dict = {
    "refresh_enabled": REFRESH_ENABLED,
    "refresh_interval_seconds": REFRESH_INTERVAL_SECONDS,
    "refresh_mode": "interval",  # "interval" 或 "daily"
    "daily_run_hour": 3,         # 每日模式：几点触发（0-23）
    "matrix_max_offset": MATRIX_MAX_OFFSET,
    "matrix_max_duration": MATRIX_MAX_DURATION,
    "matrix_concurrency": MATRIX_CONCURRENCY,
    "api_key": API_KEY,
    "base_url": BASE_URL,
    "model_name": MODEL_NAME,
    "weather_forecast_days": WEATHER_FORECAST_DAYS,
    "weather_history_days": WEATHER_HISTORY_DAYS,
}


def get_runtime_config() -> dict:
    """返回运行时配置（敏感字段已掩码）。"""
    with _config_lock:
        conf = _runtime_config.copy()
        conf["api_key"] = mask_key(conf.get("api_key", ""))
        return conf


def update_runtime_config(updates: dict) -> dict:
    """更新运行时配置，返回更新后的完整配置。"""
    with _config_lock:
        for key in (
            "refresh_enabled", "refresh_interval_seconds",
            "refresh_mode", "daily_run_hour",
            "matrix_max_offset", "matrix_max_duration", "matrix_concurrency",
            "api_key", "base_url", "model_name",
            "weather_forecast_days", "weather_history_days",
        ):
            if key in updates:
                _runtime_config[key] = updates[key]
        conf = _runtime_config.copy()
        conf["api_key"] = mask_key(conf.get("api_key", ""))
        return conf


def get_config_value(key: str):
    with _config_lock:
        return _runtime_config.get(key)


def mask_key(key: str) -> str:
    if not key or len(key) < 8:
        return "********"
    return key[:4] + "****" + key[-4:]