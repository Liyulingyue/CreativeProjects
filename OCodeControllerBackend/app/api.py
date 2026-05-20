from fastapi import APIRouter

from app.routers.auth import router as auth_router
from app.routers.devices import router as devices_router
from app.routers.configs import router as configs_router
from app.routers.sessions import router as sessions_router

api_router = APIRouter(prefix="/api")

api_router.include_router(auth_router)
api_router.include_router(devices_router)
api_router.include_router(configs_router)
api_router.include_router(sessions_router)
