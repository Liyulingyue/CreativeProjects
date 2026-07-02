"""
天气预报缓存：每日拉取未来 14 天预报 + 清理过期数据。

全部 Open-Meteo 免费 API，0 token。
"""
import sqlite3
import time
from datetime import date, timedelta
from pathlib import Path
from typing import Optional

import httpx

from .weather import fetch_weather, DailyWeather

DATA_DIR = Path(__file__).resolve().parent.parent / "data"
DB_PATH = DATA_DIR / "travelsites.db"

# 默认值（会被 config 覆盖）
FORECAST_DAYS = 14
HISTORY_DAYS = 30


def _apply_config():
    """从 app.config 读取配置（如可用），否则用默认值。"""
    global FORECAST_DAYS, HISTORY_DAYS
    try:
        from app.config import WEATHER_FORECAST_DAYS, WEATHER_HISTORY_DAYS
        FORECAST_DAYS = WEATHER_FORECAST_DAYS
        HISTORY_DAYS = WEATHER_HISTORY_DAYS
    except Exception:
        pass


def ensure_table():
    _apply_config()
    conn = sqlite3.connect(str(DB_PATH))
    conn.execute("""
        CREATE TABLE IF NOT EXISTS weather_cache (
            city TEXT NOT NULL,
            date TEXT NOT NULL,
            weather_code INTEGER,
            weather_desc TEXT,
            temp_max REAL,
            temp_min REAL,
            precipitation_mm REAL,
            precipitation_probability INTEGER,
            fetched_at TEXT,
            PRIMARY KEY (city, date)
        )
    """)
    conn.commit()
    conn.close()


def get_target_cities() -> list[tuple[str, float, float]]:
    """获取需要拉天气的城市（有坐标的都拉）。"""
    conn = sqlite3.connect(str(DB_PATH))
    rows = conn.execute(
        "SELECT name, lat, lon FROM geo_cities WHERE lat IS NOT NULL"
    ).fetchall()
    conn.close()
    return rows


def fetch_and_store(city: str, lat: float, lon: float) -> int:
    ensure_table()
    """查 Open-Meteo 未来 14 天并写入。返回写入行数。"""
    today = date.today()
    end = today + timedelta(days=FORECAST_DAYS - 1)
    try:
        weather_list = fetch_weather(lat, lon, today.isoformat(), end.isoformat())
    except Exception as e:
        print(f"  [weather] {city}: 拉取失败: {e}")
        return 0

    conn = sqlite3.connect(str(DB_PATH))
    cur = conn.cursor()
    inserted = 0
    for w in weather_list:
        cur.execute(
            """INSERT OR REPLACE INTO weather_cache
               (city, date, weather_code, weather_desc, temp_max, temp_min,
                precipitation_mm, precipitation_probability, fetched_at)
               VALUES (?, ?, ?, ?, ?, ?, ?, ?, datetime('now'))""",
            (city, w.date, w.weather_code, w.weather_desc,
             w.temp_max, w.temp_min, w.precipitation_mm,
             w.precipitation_probability),
        )
        inserted += 1
    conn.commit()
    conn.close()
    return inserted


def refresh_all() -> dict:
    """遍历所有城市拉取天气。返回统计。"""
    ensure_table()
    cities = get_target_cities()
    print(f"[weather] 拉取 {len(cities)} 城市未来 {FORECAST_DAYS} 天预报")
    ok = 0
    fail = 0
    t0 = time.time()
    for i, (city, lat, lon) in enumerate(cities):
        n = fetch_and_store(city, lat, lon)
        if n:
            ok += 1
        else:
            fail += 1
        if (i + 1) % 50 == 0:
            print(f"  [{i+1}/{len(cities)}] 成功 {ok}, 失败 {fail}")
        time.sleep(0.2)  # Open-Meteo 礼貌限流
    elapsed = time.time() - t0
    print(f"[weather] 完成：{ok}/{len(cities)}，用时 {elapsed:.0f}s")
    return {"ok": ok, "fail": fail, "elapsed": elapsed}


def cleanup_old(days: int = HISTORY_DAYS) -> int:
    ensure_table()
    """删除 N 天前的缓存。"""
    conn = sqlite3.connect(str(DB_PATH))
    cur = conn.cursor()
    cur.execute(
        "DELETE FROM weather_cache WHERE date < date('now', ?)",
        (f"-{days} days",),
    )
    conn.commit()
    deleted = cur.rowcount
    conn.close()
    if deleted > 0:
        print(f"[weather] 清理 {deleted} 条 {days} 天前的数据")
    return deleted


def get_weather(city: str, target_date: str) -> Optional[dict]:
    """供搜索时查询某城市某日天气，0 token。"""
    conn = sqlite3.connect(str(DB_PATH))
    conn.row_factory = sqlite3.Row
    row = conn.execute(
        """SELECT * FROM weather_cache WHERE city=? AND date=?""",
        (city, target_date),
    ).fetchone()
    conn.close()
    return dict(row) if row else None


if __name__ == "__main__":
    refresh_all()
    cleanup_old()
