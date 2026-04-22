from pydantic import BaseModel


class HealthResponse(BaseModel):
    car: bool
    camera: bool
