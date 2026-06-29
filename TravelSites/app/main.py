import asyncio
from contextlib import asynccontextmanager

from fastapi import FastAPI
from fastapi.middleware.cors import CORSMiddleware
from fastapi.staticfiles import StaticFiles
from pathlib import Path

from .config import REFRESH_ENABLED, REFRESH_INTERVAL_SECONDS
from .router import router
from .refresh import initial_load, start_background_refresh, get_refresh_state
from .errors import setup_exception_handlers, logger


web_dist = Path(__file__).resolve().parent.parent / "web" / "dist"


@asynccontextmanager
async def lifespan(app: FastAPI):
    # 初始化 SQLite 事实数据库（省/市/县坐标）
    try:
        from src.db import init_db, init_seed_cities
        init_db()
        init_seed_cities()
        print("[startup] SQLite 事实数据库已就绪")
    except Exception as e:
        print(f"[startup] WARN: DB init failed: {e}")

    # 填充节假日数据
    try:
        from src.holidays import fill_holidays
        fill_holidays()
    except Exception as e:
        print(f"[startup] WARN: fill_holidays failed: {e}")

    # 填充景点种子数据
    try:
        from src.db import seed_attractions
        seed_attractions()
    except Exception as e:
        print(f"[startup] WARN: seed_attractions failed: {e}")

    # 确保存在一个 admin 用户
    try:
        from src.auth import ensure_admin_user
        ensure_admin_user()
    except Exception as e:
        print(f"[startup] WARN: ensure_admin_user failed: {e}")

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

setup_exception_handlers(app)

if web_dist.exists():
    app.mount("/", StaticFiles(directory=str(web_dist), html=True))
