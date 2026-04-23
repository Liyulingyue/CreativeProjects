from .health import router as health_router
from .car import router as car_router
from .camera import router as camera_router
from .buzzer import router as buzzer_router
from .led import router as led_router
from .distance import router as distance_router
from .follow import router as follow_router

__all__ = [
    "health_router",
    "car_router",
    "camera_router",
    "buzzer_router",
    "led_router",
    "distance_router",
    "follow_router",
]
