import logging
from contextlib import asynccontextmanager

from fastapi import FastAPI
from fastapi.middleware.cors import CORSMiddleware
from sqlalchemy import text

from app.config import settings
from app.database import engine, Base
from app.api import api_router


MIGRATIONS = [
    ("users", "plan", "ALTER TABLE users ADD COLUMN plan VARCHAR(16) DEFAULT 'member'"),
    ("containers", "opencode_username", "ALTER TABLE containers ADD COLUMN opencode_username VARCHAR(64)"),
    ("containers", "opencode_password", "ALTER TABLE containers ADD COLUMN opencode_password VARCHAR(128)"),
    ("containers", "filebrowser_url", "ALTER TABLE containers ADD COLUMN filebrowser_url VARCHAR(512)"),
    ("containers", "fb_username", "ALTER TABLE containers ADD COLUMN fb_username VARCHAR(64)"),
    ("containers", "fb_password", "ALTER TABLE containers ADD COLUMN fb_password VARCHAR(128)"),
]


@asynccontextmanager
async def lifespan(app: FastAPI):
    async with engine.begin() as conn:
        await conn.run_sync(Base.metadata.create_all)

    for table, column, sql in MIGRATIONS:
        try:
            async with engine.begin() as conn:
                await conn.execute(text(f"SELECT {column} FROM {table} LIMIT 1"))
        except Exception:
            async with engine.begin() as conn:
                await conn.execute(text(sql))
                logging.info(f"[Migration] Added column {column} to table {table}")

    yield


app = FastAPI(
    title=settings.APP_NAME,
    lifespan=lifespan
)

app.add_middleware(
    CORSMiddleware,
    allow_origins=["*"],
    allow_credentials=True,
    allow_methods=["*"],
    allow_headers=["*"],
)

app.include_router(api_router)


@app.get("/health")
async def health():
    return {"status": "ok"}
