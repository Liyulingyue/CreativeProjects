from datetime import datetime

from pydantic import BaseModel, ConfigDict


class DeviceBase(BaseModel):
    device_id: str
    device_name: str | None = None


class DeviceCreate(DeviceBase):
    pass


class DeviceResponse(DeviceBase):
    id: str
    user_id: str
    last_login: datetime

    model_config = ConfigDict(from_attributes=True)
