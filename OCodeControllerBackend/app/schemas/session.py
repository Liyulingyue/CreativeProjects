from datetime import datetime

from pydantic import BaseModel, ConfigDict


class SessionBase(BaseModel):
    id: str
    title: str | None = None


class SessionCreate(SessionBase):
    pass


class SessionUpdate(BaseModel):
    title: str | None = None


class SessionResponse(SessionBase):
    user_id: str
    created_at: datetime
    updated_at: datetime

    model_config = ConfigDict(from_attributes=True)
