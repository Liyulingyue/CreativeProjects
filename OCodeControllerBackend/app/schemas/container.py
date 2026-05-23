from datetime import datetime

from pydantic import BaseModel, ConfigDict


class ContainerResponse(BaseModel):
    id: str
    name: str
    port: int | None = None
    opencode_url: str | None = None
    opencode_token: str | None = None
    status: str
    created_at: datetime
    updated_at: datetime

    model_config = ConfigDict(from_attributes=True)


class ContainerCreate(BaseModel):
    name: str = "未命名环境"
