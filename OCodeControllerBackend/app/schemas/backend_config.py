from datetime import datetime

from pydantic import BaseModel, ConfigDict


class BackendConfigBase(BaseModel):
    backend_url: str
    username: str | None = None
    auth_token: str | None = None
    remark: str | None = None


class BackendConfigCreate(BackendConfigBase):
    pass


class BackendConfigUpdate(BaseModel):
    backend_url: str | None = None
    username: str | None = None
    auth_token: str | None = None
    remark: str | None = None


class BackendConfigSync(BaseModel):
    id: str
    backend_url: str
    username: str | None = None
    auth_token: str | None = None
    remark: str | None = None


class BackendConfigResponse(BackendConfigBase):
    id: str
    user_id: str
    is_active: bool
    updated_at: datetime

    model_config = ConfigDict(from_attributes=True)
