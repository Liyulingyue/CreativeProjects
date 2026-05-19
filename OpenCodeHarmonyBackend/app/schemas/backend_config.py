from datetime import datetime

from pydantic import BaseModel, ConfigDict


class BackendConfigBase(BaseModel):
    backend_url: str
    auth_token: str | None = None
    remark: str | None = None
    is_active: bool = False


class BackendConfigCreate(BackendConfigBase):
    pass


class BackendConfigUpdate(BaseModel):
    backend_url: str | None = None
    auth_token: str | None = None
    remark: str | None = None
    is_active: bool | None = None


class BackendConfigResponse(BackendConfigBase):
    id: str
    user_id: str
    updated_at: datetime

    model_config = ConfigDict(from_attributes=True)
