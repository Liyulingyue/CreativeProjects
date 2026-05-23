from datetime import datetime

from pydantic import BaseModel, ConfigDict


class SessionBase(BaseModel):
    id: str
    title: str | None = None


class SessionCreate(SessionBase):
    pass


class SessionUpdate(BaseModel):
    title: str | None = None


class SessionSync(BaseModel):
    id: str
    title: str | None = None
    backend_url: str | None = None
    backend_id: str | None = None
    directory: str | None = None
    remote_session_id: str | None = None
    preferred_model: str | None = None
    last_access: datetime | None = None


class SessionResponse(SessionBase):
    user_id: str
    backend_url: str | None = None
    backend_id: str | None = None
    directory: str | None = None
    remote_session_id: str | None = None
    preferred_model: str | None = None
    last_access: datetime | None = None
    created_at: datetime
    updated_at: datetime

    model_config = ConfigDict(from_attributes=True)
