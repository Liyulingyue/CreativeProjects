"""全局配置：LLM 走 OpenAI 兼容接口（ChatGLM / DeepSeek / OpenAI 均可）。"""
from pathlib import Path

from pydantic_settings import BaseSettings, SettingsConfigDict

BASE_DIR = Path(__file__).resolve().parent.parent  # backend/

class Settings(BaseSettings):
    model_config = SettingsConfigDict(env_file=BASE_DIR / ".env", env_file_encoding="utf-8", extra="ignore")

    # 不设置 LLM_API_KEY 时，所有依赖大模型的能力（生成测评 / LLM 报告 / 合理性点评）自动降级
    llm_api_key: str = ""
    llm_base_url: str = "https://open.bigmodel.cn/api/paas/v4"  # 智谱 ChatGLM
    llm_model: str = "glm-4-flash"
    llm_timeout: float = 60.0

    store_dir: Path = BASE_DIR / "data"

settings = Settings()
