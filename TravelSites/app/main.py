import asyncio
from contextlib import asynccontextmanager

from fastapi import FastAPI
from fastapi.middleware.cors import CORSMiddleware
from fastapi.staticfiles import StaticFiles
from pathlib import Path

from .config import REFRESH_ENABLED, REFRESH_INTERVAL_SECONDS, WEATHER_REFRESH_ON_STARTUP
from .router import router
from .refresh import initial_load, start_background_refresh, get_refresh_state
from .errors import setup_exception_handlers, logger


web_dist = Path(__file__).resolve().parent.parent / "web" / "dist"


@asynccontextmanager
async def lifespan(app: FastAPI):
    # 初始化 SQLite 事实数据库（省/市/县坐标）
    try:
        from src.db import init_db, init_seed_cities, migrate_matrix_schema, migrate_add_input_metadata
        init_db()
        init_seed_cities()
        migrate_matrix_schema()
        migrate_add_input_metadata()
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

    # 清理过期数据
    try:
        from src.db import cleanup_old_logs, cleanup_old_cache
        cleanup_old_logs(90)
        cleanup_old_cache(30)
    except Exception as e:
        print(f"[startup] WARN: cleanup failed: {e}")

    # 天气预报缓存（启动时拉一次，保证 DB 有数据）
    try:
        from src.weather_cache import refresh_all, cleanup_old
        cleanup_old(30)
        if WEATHER_REFRESH_ON_STARTUP:
            refresh_all()
            print(f"[startup] 天气预报已缓存")
        else:
            print(f"[startup] 天气预报未拉取（WEATHER_REFRESH_ON_STARTUP=false）")
    except Exception as e:
        print(f"[startup] WARN: weather cache failed: {e}")

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
