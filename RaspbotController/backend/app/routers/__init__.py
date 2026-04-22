from fastapi import APIRouter
from .health import router as health_router
from .car import router as car_router
from .camera import router as camera_router

__all__ = ["health_router", "car_router", "camera_router"]
