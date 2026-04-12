import json
import os
from pathlib import Path
from dotenv import load_dotenv

load_dotenv()

BASE_DIR = Path(__file__).resolve().parent.parent
CONFIG_FILE = BASE_DIR / "config.json"

DEFAULT_CONFIG = {
    "MODEL_KEY": "",
    "MODEL_URL": "https://api.openai.com/v1",
    "MODEL_NAME": "gpt-4",
    "BAIDU_AISTUDIO_KEY": ""
}

def load_config() -> dict:
    if CONFIG_FILE.exists():
        with open(CONFIG_FILE, "r", encoding="utf-8") as f:
            return json.load(f)
    return DEFAULT_CONFIG.copy()

def save_config(config: dict) -> None:
    with open(CONFIG_FILE, "w", encoding="utf-8") as f:
        json.dump(config, f, indent=2, ensure_ascii=False)

def get_config() -> dict:
    config = load_config()
    model_url = os.getenv("MODEL_URL", config.get("MODEL_URL", DEFAULT_CONFIG["MODEL_URL"]))
    model_name = os.getenv("MODEL_NAME", config.get("MODEL_NAME", DEFAULT_CONFIG["MODEL_NAME"]))
    return {
        "MODEL_KEY": "",
        "MODEL_URL": model_url,
        "MODEL_NAME": model_name,
        "BAIDU_AISTUDIO_KEY": ""
    }

def update_config(url: str = None, key: str = None, model: str = None, baidu_key: str = None) -> dict:
    config = load_config()
    if url is not None:
        config["MODEL_URL"] = url
    if key is not None and key != "":
        config["MODEL_KEY"] = key
    if model is not None:
        config["MODEL_NAME"] = model
    if baidu_key is not None:
        config["BAIDU_AISTUDIO_KEY"] = baidu_key
    save_config(config)

    if key is not None and key != "":
        os.environ["MODEL_KEY"] = key
    if url is not None:
        os.environ["MODEL_URL"] = url
    if model is not None:
        os.environ["MODEL_NAME"] = model
    if baidu_key is not None:
        os.environ["BAIDU_AISTUDIO_KEY"] = baidu_key

    from app.services.ai_service import reload_config
    reload_config()

    return {
        "MODEL_KEY": "",
        "MODEL_URL": config["MODEL_URL"],
        "MODEL_NAME": config["MODEL_NAME"],
        "BAIDU_AISTUDIO_KEY": config.get("BAIDU_AISTUDIO_KEY", "")
    }
