import asyncio
from contextlib import asynccontextmanager

from fastapi import FastAPI
from fastapi.middleware.cors import CORSMiddleware
from fastapi.staticfiles import StaticFiles
from pathlib import Path

from .config import REFRESH_ENABLED, REFRESH_INTERVAL_SECONDS
from .router import router
from .refresh import initial_load, start_background_refresh, get_refresh_state


web_dist = Path(__file__).resolve().parent.parent / "web" / "dist"


@asynccontextmanager
async def lifespan(app: FastAPI):
    await initial_load()

    if REFRESH_ENABLED:
        asyncio.create_task(start_background_refresh(REFRESH_INTERVAL_SECONDS))
        print(f"[startup] 定时刷新已启用，间隔 {REFRESH_INTERVAL_SECONDS} 秒")
    else:
        print("[startup] 定时刷新已禁用（REFRESH_ENABLED=false）")

    yield


app = FastAPI(
    title="TravelSites API",
    description="时空驱动的旅游目的地发现平台",
    version="1.0.0",
    lifespan=lifespan,
)

app.add_middleware(
    CORSMiddleware,
    allow_origins=["*"],
    allow_credentials=True,
    allow_methods=["*"],
    allow_headers=["*"],
)

app.include_router(router, prefix="/api")

if web_dist.exists():
    app.mount("/", StaticFiles(directory=str(web_dist), html=True))
