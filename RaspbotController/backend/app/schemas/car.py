from pydantic import BaseModel, Field


class MoveRequest(BaseModel):
    direction: str = Field(..., description="forward|backward|left|right|spin_left|spin_right|stop")
    speed: int = Field(50, ge=0, le=100, description="Speed 0-100")


class MoveResponse(BaseModel):
    status: str
    direction: str
    speed: int


class StopResponse(BaseModel):
    status: str


class ManualControlRequest(BaseModel):
    l_speed: int = Field(..., ge=-100, le=100, description="Left motor speed -100 to 100")
    r_speed: int = Field(..., ge=-100, le=100, description="Right motor speed -100 to 100")


class ManualControlResponse(BaseModel):
    status: str
    l_speed: int
    r_speed: int


class ServoControlRequest(BaseModel):
    id: int = Field(1, ge=0, le=3, description="Servo ID")
    angle: int = Field(..., ge=0, le=180, description="Angle 0-180")


class ServoControlResponse(BaseModel):
    status: str
    id: int
    angle: int
