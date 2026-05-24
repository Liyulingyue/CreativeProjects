from pydantic_settings import BaseSettings


class Settings(BaseSettings):
    APP_NAME: str = "OpenCodeHarmonyBackend"
    SECRET_KEY: str = "your-secret-key-change-in-production"
    ALGORITHM: str = "HS256"
    ACCESS_TOKEN_EXPIRE_MINUTES: int = 60 * 24 * 7

    DATABASE_URL: str = "sqlite+aiosqlite:///./opencode_harmony.db"

    DOCKER_IMAGE: str = "opencode-service:latest"
    SERVER_HOST: str = "localhost"
    REDEEM_MEMBER: str = "MEMBER2025"
    REDEEM_VIP: str = "VIP2025"
    DOCKER_SUDO_PASSWORD: str = ""

    class Config:
        env_file = ".env"


settings = Settings()
