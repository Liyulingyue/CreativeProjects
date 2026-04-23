from fastapi import FastAPI
from fastapi.middleware.cors import CORSMiddleware
from .routers import (
    health_router,
    car_router,
    camera_router,
    buzzer_router,
    led_router,
    distance_router,
    follow_router,
)


def create_app() -> FastAPI:
    app = FastAPI(
        title="Raspbot Controller API",
        version="1.0.0",
        description="API for controlling the Yahboom Raspberry Pi robot",
    )

    app.add_middleware(
        CORSMiddleware,
        allow_origins=["*"],
        allow_credentials=True,
        allow_methods=["*"],
        allow_headers=["*"],
    )

    app.include_router(health_router)
    app.include_router(car_router)
    app.include_router(camera_router)
    app.include_router(buzzer_router)
    app.include_router(led_router)
    app.include_router(distance_router)
    app.include_router(follow_router)

    @app.get("/")
    async def root():
        return {"status": "ok", "message": "Raspbot Controller API"}

    return app


def get_app() -> FastAPI:
    return create_app()
