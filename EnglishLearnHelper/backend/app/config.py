import os
from pathlib import Path

BASE_DIR = Path(__file__).resolve().parent.parent
DATA_DIR = BASE_DIR / "Data" / "IELTS"

MODEL_KEY = os.getenv("MODEL_KEY", "")
MODEL_URL = os.getenv("MODEL_URL", "https://api.openai.com/v1")
MODEL_NAME = os.getenv("MODEL_NAME", "gpt-4")
