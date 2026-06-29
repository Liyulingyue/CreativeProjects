"""
SQLite 数据库初始化与查询。

Schema 设计原则：
- 表名带前缀（cities_/attractions_/trips_）便于区分域
- 字段冗余常见查询字段（避免 JOIN 热点）
- 显式外键 + 索引
- JSON 列存灵活扩展字段（tags、aliases 等）
- 启动加载到内存缓存，热查询走内存

TODO: 后续扩展
- attractions 表：景点 POI（接高德 API）
- transport_hubs 表：高铁站/机场坐标
- holidays 表：节假日 + 调休
- trips_cache 表：matrix 生成结果（替代 JSON 文件）
"""
import json
import sqlite3
from pathlib import Path
from threading import RLock
from typing import Optional


DATA_DIR = Path(__file__).resolve().parent.parent / "data"
DB_PATH = DATA_DIR / "travelsites.db"
JSON_PATH = DATA_DIR / "china_regions_enriched.json"


# ---------- Schema ----------

SCHEMA = """
-- 省份
CREATE TABLE IF NOT EXISTS geo_provinces (
    code TEXT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    lat REAL,
    lon REAL
);

-- 城市
CREATE TABLE IF NOT EXISTS geo_cities (
    code TEXT PRIMARY KEY,
    province_code TEXT NOT NULL,
    province_name TEXT NOT NULL,
    name TEXT NOT NULL,
    lat REAL,
    lon REAL,
    is_municipality INTEGER DEFAULT 0,
    aliases TEXT,           -- JSON array, 别名
    population INTEGER,     -- 万人（可后续接 API）
    UNIQUE(province_name, name)
);
CREATE INDEX IF NOT EXISTS idx_cities_name ON geo_cities(name);
CREATE INDEX IF NOT EXISTS idx_cities_province ON geo_cities(province_name);

-- 县/区
CREATE TABLE IF NOT EXISTS geo_counties (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL,
    city_code TEXT NOT NULL,
    city_name TEXT NOT NULL,
    province_name TEXT NOT NULL,
    lat REAL,
    lon REAL,
    UNIQUE(city_name, name)
);
CREATE INDEX IF NOT EXISTS idx_counties_city ON geo_counties(city_name);
CREATE INDEX IF NOT EXISTS idx_counties_name ON geo_counties(name);

-- 节假日（可扩展性设计：支持地区/人群/来源维度）
CREATE TABLE IF NOT EXISTS holiday_calendar (
    date TEXT NOT NULL,        -- YYYY-MM-DD
    name TEXT NOT NULL,         -- 春节、国庆等
    type TEXT NOT NULL,         -- public / observed / makeup / weekend
    impact_level INTEGER DEFAULT 0,  -- 0=正常, 1=小长假, 2=大长假, 3=春节/国庆
    region_code TEXT DEFAULT NULL,    -- NULL=全国通用, "HK"/"MO"/"TIB" 等
    demographic TEXT DEFAULT NULL,    -- NULL=全人群, "student"/"worker"
    source TEXT DEFAULT 'state_council', -- 数据来源标识
    PRIMARY KEY (date, region_code, demographic)
);
CREATE INDEX IF NOT EXISTS idx_holiday_date ON holiday_calendar(date);
CREATE INDEX IF NOT EXISTS idx_holiday_region ON holiday_calendar(region_code);

-- Matrix 缓存（替代 JSON 文件，未来扩展）
CREATE TABLE IF NOT EXISTS trip_matrix_cache (
    city TEXT NOT NULL,
    start_date TEXT NOT NULL,
    duration INTEGER NOT NULL,
    end_date TEXT NOT NULL,
    score INTEGER,
    recommendation TEXT,
    weather_summary TEXT,
    full_result TEXT,  -- JSON
    input_metadata TEXT,  -- JSON：{start_date, duration, model, lite, weather_hash}，用于 cache 命中判断
    generated_at TEXT,
    PRIMARY KEY (city, start_date, duration)
);
CREATE INDEX IF NOT EXISTS idx_matrix_date ON trip_matrix_cache(start_date, end_date);

-- 种子城市配置（管理员可动态修改）
CREATE TABLE IF NOT EXISTS seed_config (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

-- 系统统计（token 用量、生成历史）
CREATE TABLE IF NOT EXISTS generation_log (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    city TEXT NOT NULL,
    started_at TEXT NOT NULL,
    finished_at TEXT,
    cells_total INTEGER,
    cells_success INTEGER,
    duration_seconds REAL,
    source TEXT DEFAULT 'scheduled'  -- 'scheduled' | 'manual' | 'incremental'
);
CREATE INDEX IF NOT EXISTS idx_log_city ON generation_log(city, started_at);

-- 用户表（支持未来协作扩展）
CREATE TABLE IF NOT EXISTS users (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    username TEXT NOT NULL UNIQUE,
    email TEXT UNIQUE,
    password_hash TEXT NOT NULL,
    role TEXT NOT NULL DEFAULT 'user',  -- 'user' | 'admin'
    display_name TEXT,
    created_at TEXT NOT NULL,
    last_login_at TEXT,
    metadata TEXT  -- JSON: 偏好、设置等可扩展字段
);
CREATE INDEX IF NOT EXISTS idx_users_role ON users(role);

-- 用户会话（简化版：直接存 token 而非 JWT）
CREATE TABLE IF NOT EXISTS user_sessions (
    token TEXT PRIMARY KEY,
    user_id INTEGER NOT NULL,
    created_at TEXT NOT NULL,
    expires_at TEXT NOT NULL,
    FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS idx_sessions_user ON user_sessions(user_id);
CREATE INDEX IF NOT EXISTS idx_sessions_expires ON user_sessions(expires_at);

-- 景点库（真实数据，优先级高于 LLM 生成）
CREATE TABLE IF NOT EXISTS attractions (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL,
    city TEXT NOT NULL,         -- 所属城市
    category TEXT,              -- 古迹/山岳/博物馆/公园/美食街/寺庙/园林
    lat REAL,
    lon REAL,
    address TEXT,
    rating REAL,                -- 1-5
    ticket_price REAL,          -- 元
    suggested_hours REAL,       -- 建议游览时长
    open_hours TEXT,            -- e.g. "08:00-17:00"
    tags TEXT,                  -- JSON array
    description TEXT,
    source TEXT DEFAULT 'seed', -- 'seed' | 'amap' | 'baidu' | 'user'
    verified INTEGER DEFAULT 1, -- 是否经过验证（API 存在）
    created_at TEXT,
    UNIQUE(name, city)
);
CREATE INDEX IF NOT EXISTS idx_attractions_city ON attractions(city);
CREATE INDEX IF NOT EXISTS idx_attractions_category ON attractions(category);

-- 景点别名/同义词（用于搜索匹配）
CREATE TABLE IF NOT EXISTS attraction_aliases (
    attraction_id INTEGER NOT NULL,
    alias TEXT NOT NULL,
    PRIMARY KEY (alias),
    FOREIGN KEY (attraction_id) REFERENCES attractions(id) ON DELETE CASCADE
);
"""


# ---------- 数据库连接 ----------

_lock = RLock()
_conn: Optional[sqlite3.Connection] = None


def get_conn() -> sqlite3.Connection:
    global _conn
    with _lock:
        if _conn is None:
            DATA_DIR.mkdir(parents=True, exist_ok=True)
            _conn = sqlite3.connect(str(DB_PATH), check_same_thread=False)
            _conn.row_factory = sqlite3.Row
            _conn.execute("PRAGMA foreign_keys = ON")
            _conn.executescript(SCHEMA)
            _conn.commit()
        return _conn


def init_db():
    """初始化数据库并从 JSON 加载数据（如果表为空）。"""
    conn = get_conn()
    cur = conn.cursor()

    cur.execute("SELECT COUNT(*) FROM geo_provinces")
    if cur.fetchone()[0] > 0:
        return  # 已初始化

    if not JSON_PATH.exists():
        print(f"[db] WARN: {JSON_PATH} not found, skip loading")
        return

    with open(JSON_PATH, "r", encoding="utf-8") as f:
        data = json.load(f)

    provinces = []
    cities = []
    counties = []

    for prov_name, prov in data.items():
        provinces.append((
            prov.get("code", ""),
            prov_name,
            prov.get("latitude"),
            prov.get("longitude"),
        ))

        for city_name, city in prov.get("cities", {}).items():
            cities.append((
                city.get("code", ""),
                prov.get("code", ""),
                prov_name,
                city_name,
                city.get("latitude"),
                city.get("longitude"),
                1 if city.get("is_municipality") else 0,
                None,
                None,
            ))

            for county_name in city.get("counties", []):
                counties.append((
                    county_name,
                    city.get("code", ""),
                    city_name,
                    prov_name,
                    city.get("latitude"),
                    city.get("longitude"),
                ))

    cur.executemany(
        "INSERT OR IGNORE INTO geo_provinces (code, name, lat, lon) VALUES (?,?,?,?)",
        provinces,
    )
    cur.executemany(
        """INSERT OR IGNORE INTO geo_cities
           (code, province_code, province_name, name, lat, lon, is_municipality, aliases, population)
           VALUES (?,?,?,?,?,?,?,?,?)""",
        cities,
    )
    cur.executemany(
        """INSERT OR IGNORE INTO geo_counties
           (name, city_code, city_name, province_name, lat, lon)
           VALUES (?,?,?,?,?,?)""",
        counties,
    )

    conn.commit()
    print(f"[db] Loaded {len(provinces)} provinces, {len(cities)} cities, {len(counties)} counties")


def migrate_matrix_schema():
    """如果 trip_matrix_cache 表还在用旧 schema（start_offset 列），则迁移到新 schema。"""
    import json
    from datetime import datetime, timedelta

    conn = get_conn()
    cur = conn.cursor()

    # 检测旧表是否有 start_offset 列
    try:
        cur.execute("SELECT start_offset FROM trip_matrix_cache LIMIT 1")
    except Exception:
        return  # 新表，无 start_offset 列，无需迁移

    print("[db] 检测到旧版 trip_matrix_cache schema，开始迁移…")

    # 读取所有旧数据
    rows = cur.execute(
        """SELECT city, start_offset, duration, start_date, end_date,
                  score, recommendation, weather_summary, full_result, generated_at
           FROM trip_matrix_cache"""
    ).fetchall()

    if not rows:
        print("[db] 旧表无数据，直接重建")
        cur.execute("DROP TABLE IF EXISTS trip_matrix_cache_tmp")
        cur.execute(
            """CREATE TABLE trip_matrix_cache_tmp AS
               SELECT city, start_date, duration, end_date,
                      score, recommendation, weather_summary, full_result, generated_at
               FROM trip_matrix_cache LIMIT 0"""
        )
        cur.execute("DROP TABLE trip_matrix_cache")
        cur.execute("ALTER TABLE trip_matrix_cache_tmp RENAME TO trip_matrix_cache")
        cur.execute("CREATE UNIQUE INDEX IF NOT EXISTS idx_matrix_pk ON trip_matrix_cache(city, start_date, duration)")
        conn.commit()
        print("[db] 迁移完成（新表无数据）")
        return

    # 写入新表
    cur.execute("DROP TABLE IF EXISTS trip_matrix_cache_new")
    cur.execute(
        """CREATE TABLE trip_matrix_cache_new (
            city TEXT NOT NULL,
            start_date TEXT NOT NULL,
            duration INTEGER NOT NULL,
            end_date TEXT NOT NULL,
            score INTEGER,
            recommendation TEXT,
            weather_summary TEXT,
            full_result TEXT,
            generated_at TEXT,
            PRIMARY KEY (city, start_date, duration)
        )"""
    )

    migrated = 0
    for r in rows:
        start_offset = r[1]
        generated_at = r[9]
        if generated_at and start_offset is not None:
            try:
                gen_date = datetime.fromisoformat(generated_at).date()
                sd = datetime.strptime(r[3], "%Y-%m-%d").date()
                computed_offset = (sd - gen_date).days
            except Exception:
                computed_offset = start_offset
        else:
            computed_offset = start_offset

        cur.execute(
            """INSERT OR REPLACE INTO trip_matrix_cache_new
               (city, start_date, duration, end_date, score, recommendation,
                weather_summary, full_result, generated_at)
               VALUES (?,?,?,?,?,?,?,?,?)""",
            (r[0], r[3], r[2], r[4], r[5], r[6], r[7], r[8], r[9]),
        )
        migrated += 1

    cur.execute("DROP TABLE trip_matrix_cache")
    cur.execute("ALTER TABLE trip_matrix_cache_new RENAME TO trip_matrix_cache")
    cur.execute("CREATE INDEX IF NOT EXISTS idx_matrix_date ON trip_matrix_cache(start_date, end_date)")
    conn.commit()
    print(f"[db] 迁移完成，共 {migrated} 条数据")


def migrate_add_input_metadata():
    """为 trip_matrix_cache 表加 input_metadata 列（如不存在），并清理旧的 input_fingerprint。"""
    conn = get_conn()
    cur = conn.cursor()

    # 检查是否已有 input_metadata
    try:
        cur.execute("SELECT input_metadata FROM trip_matrix_cache LIMIT 1")
        return  # 已有
    except Exception:
        pass

    # 检查是否还有旧列 input_fingerprint
    has_legacy = False
    try:
        cur.execute("SELECT input_fingerprint FROM trip_matrix_cache LIMIT 1")
        has_legacy = True
    except Exception:
        pass

    cur.execute("ALTER TABLE trip_matrix_cache ADD COLUMN input_metadata TEXT")
    conn.commit()

    if has_legacy:
        # 旧列存在但语义不同（是 hash，不是 JSON metadata），直接清空
        # 让旧 cell 走一次重生成，沉淀出正确的 JSON metadata
        cur.execute("UPDATE trip_matrix_cache SET input_metadata=NULL WHERE input_metadata IS NULL")
        conn.commit()
        print("[db] 已添加 input_metadata 列（旧 input_fingerprint 留空，待重生成覆盖）")
    else:
        print("[db] 已为 trip_matrix_cache 添加 input_metadata 列")


def seed_attractions():
    """
    加载景点数据到 DB。
    数据源：data/attractions_seed.json（JSON 格式）
    第一次启动时自动从 JSON 加载。
    """
    import json
    from datetime import datetime
    from pathlib import Path

    json_path = Path(__file__).resolve().parent.parent / "data" / "attractions_seed.json"
    if not json_path.exists():
        print(f"[attractions] {json_path} 不存在，跳过")
        return

    with open(json_path, "r", encoding="utf-8") as f:
        data = json.load(f)

    conn = get_conn()
    cur = conn.cursor()
    cur.execute("DELETE FROM attractions WHERE source='seed'")

    now = datetime.now().isoformat()
    inserted = 0
    for city, items in data.items():
        for a in items:
            tags_json = json.dumps(a.get("tags", []), ensure_ascii=False)
            cur.execute(
                """INSERT OR IGNORE INTO attractions
                   (name, city, category, lat, lon, rating, suggested_hours, tags, source, verified, created_at)
                   VALUES (?,?,?,?,?,?,?,?,'seed',1,?)""",
                (a["name"], city, a.get("category"), a.get("lat"), a.get("lon"),
                 a.get("rating"), a.get("hours"), tags_json, now),
            )
            if cur.rowcount > 0:
                inserted += 1
    conn.commit()
    total = cur.execute("SELECT COUNT(*) FROM attractions").fetchone()[0]
    cities_count = cur.execute("SELECT COUNT(DISTINCT city) FROM attractions").fetchone()[0]
    print(f"[attractions] 新增 {inserted} 个景点，共 {total} 个，{cities_count} 城市")

# ---------- 查询 API ----------

def lookup_city(name: str) -> Optional[tuple[float, float]]:
    """按城市名查坐标（支持模糊匹配）。"""
    row = get_conn().execute(
        "SELECT lat, lon FROM geo_cities WHERE name=? AND lat IS NOT NULL",
        (name,),
    ).fetchone()
    if row:
        return (row["lat"], row["lon"])

    # LIKE 前缀匹配（如"大理"→"大理白族自治州"）
    row = get_conn().execute(
        "SELECT lat, lon FROM geo_cities WHERE name LIKE ? AND lat IS NOT NULL ORDER BY LENGTH(name) LIMIT 1",
        (f"{name}%",),
    ).fetchone()
    if row:
        return (row["lat"], row["lon"])

    return None


def lookup_county(name: str) -> Optional[tuple[str, str, float, float]]:
    """按县名查 (省, 市, lat, lon)。"""
    row = get_conn().execute(
        "SELECT province_name, city_name, lat, lon FROM geo_counties WHERE name=?",
        (name,),
    ).fetchone()
    if row:
        return (row["province_name"], row["city_name"], row["lat"] or 0, row["lon"] or 0)
    return None


def lookup_origin_county(province: str, city: str, county: str) -> Optional[tuple[float, float]]:
    """按 (省, 市, 县) 查坐标。无精确坐标时 fallback 到市。"""
    if county:
        row = get_conn().execute(
            """SELECT lat, lon FROM geo_counties
               WHERE province_name=? AND city_name=? AND name=?
               AND lat IS NOT NULL AND lon IS NOT NULL""",
            (province, city, county),
        ).fetchone()
        if row:
            return (row["lat"], row["lon"])

    if city:
        row = get_conn().execute(
            "SELECT lat, lon FROM geo_cities WHERE province_name=? AND name=? AND lat IS NOT NULL",
            (province, city),
        ).fetchone()
        if row:
            return (row["lat"], row["lon"])

    return None


def get_all_cities() -> list[str]:
    """获取所有有坐标的城市名（用于 SEED_CITIES 列表）。"""
    rows = get_conn().execute(
        "SELECT DISTINCT name FROM geo_cities WHERE lat IS NOT NULL AND lon IS NOT NULL ORDER BY name"
    ).fetchall()
    return [r["name"] for r in rows]


def get_holiday_score(date_str: str) -> tuple[str, int]:
    """返回节假日名称和影响级别（0=正常, 1=小长假, 2=大长假）。"""
    row = get_conn().execute(
        "SELECT name, impact_level FROM holiday_calendar WHERE date=?",
        (date_str,),
    ).fetchone()
    if row:
        return (row["name"], row["impact_level"])
    return ("", 0)


# ---------- 种子城市配置 ----------

DEFAULT_SEED_CITIES = [
    "济南", "大同", "青岛", "烟台", "威海",
    "杭州", "苏州", "南京", "宁波", "绍兴",
    "厦门", "福州", "泉州", "霞浦",
    "西安", "成都", "重庆", "昆明", "大理", "丽江",
    "桂林", "北海", "涠洲岛",
    "三亚", "海口", "万宁",
    "黄山", "宏村", "婺源", "千岛湖",
    "敦煌", "张掖", "嘉峪关",
    "拉萨", "林芝",
]


def init_seed_cities():
    """
    初始化种子城市列表到 DB。

    同步策略（按环境变量 SEED_CITIES_SYNC 决定）：
    - "true"：DB 强制与 .env 同步（管理员修改会被覆盖）
    - "false"（默认）：DB 已有值则保留
    - DB 为空：始终用 .env 初始化
    """
    import os
    from app.config import SEED_CITIES as ENV_CITIES

    sync = os.getenv("SEED_CITIES_SYNC", "false").lower() in ("true", "1", "yes")

    conn = get_conn()
    cur = conn.cursor()
    cur.execute("SELECT value FROM seed_config WHERE key='cities'")
    row = cur.fetchone()

    if row is None:
        cities = ENV_CITIES or DEFAULT_SEED_CITIES
        cur.execute(
            "INSERT INTO seed_config (key, value) VALUES (?, ?)",
            ("cities", json.dumps(cities, ensure_ascii=False)),
        )
        conn.commit()
        print(f"[seed] 初始化 {len(cities)} 城市（来源：.env）")
    elif sync and ENV_CITIES and set(ENV_CITIES) != set(json.loads(row["value"])):
        db_cities = json.loads(row["value"])
        only_env = set(ENV_CITIES) - set(db_cities)
        only_db = set(db_cities) - set(ENV_CITIES)
        cur.execute(
            "UPDATE seed_config SET value=? WHERE key='cities'",
            (json.dumps(ENV_CITIES, ensure_ascii=False),),
        )
        conn.commit()
        print(f"[seed] 强制同步 {len(ENV_CITIES)} 城市（来源：.env）")
        if only_env:
            print(f"   新增: {sorted(only_env)}")
        if only_db:
            print(f"   移除: {sorted(only_db)}")
    elif ENV_CITIES and set(ENV_CITIES) != set(json.loads(row["value"])):
        db_cities = json.loads(row["value"])
        only_env = set(ENV_CITIES) - set(db_cities)
        only_db = set(db_cities) - set(ENV_CITIES)
        print(f"[seed] ⚠️  .env 与 DB 不一致（DB 优先）:")
        if only_env:
            print(f"   .env 独有: {sorted(only_env)}")
        if only_db:
            print(f"   DB 独有:   {sorted(only_db)}")
        print(f"   同步方法: 设置 SEED_CITIES_SYNC=true 强制覆盖")
    else:
        print(f"[seed] 已是最新（{len(ENV_CITIES)} 城市，DB 与 .env 一致）")

def get_seed_cities() -> list[str]:
    """从 DB 读取当前生效的种子城市列表。"""
    conn = get_conn()
    row = conn.execute("SELECT value FROM seed_config WHERE key='cities'").fetchone()
    if row:
        return json.loads(row["value"])
    return DEFAULT_SEED_CITIES


def set_seed_cities(cities: list[str]) -> None:
    """更新种子城市列表（管理员操作）。"""
    conn = get_conn()
    conn.execute(
        "INSERT OR REPLACE INTO seed_config (key, value) VALUES (?, ?)",
        ("cities", json.dumps(cities, ensure_ascii=False)),
    )
    conn.commit()


def get_city_attractions(city: str, limit: int = 20) -> list[dict]:
    """获取某城市所有景点。"""
    conn = get_conn()
    conn.row_factory = sqlite3.Row
    rows = conn.execute(
        """SELECT name, category, lat, lon, rating, suggested_hours, tags, address
           FROM attractions WHERE city=? ORDER BY rating DESC LIMIT ?""",
        (city, limit),
    ).fetchall()
    results = []
    for r in rows:
        item = dict(r)
        try:
            item["tags"] = json.loads(item["tags"]) if item["tags"] else []
        except Exception:
            item["tags"] = []
        results.append(item)
    return results


def get_attraction_by_name(name: str, city: str) -> Optional[dict]:
    """按名字查景点（精确匹配）。"""
    conn = get_conn()
    conn.row_factory = sqlite3.Row
    row = conn.execute(
        "SELECT * FROM attractions WHERE name=? AND city=?",
        (name, city),
    ).fetchone()
    if not row:
        return None
    item = dict(row)
    try:
        item["tags"] = json.loads(item["tags"]) if item["tags"] else []
    except Exception:
        item["tags"] = []
    return item


def search_attractions(query: str, city: Optional[str] = None, limit: int = 20) -> list[dict]:
    """按名字模糊搜索景点。"""
    conn = get_conn()
    conn.row_factory = sqlite3.Row
    sql = "SELECT name, city, category, rating FROM attractions WHERE name LIKE ?"
    params: list = [f"%{query}%"]
    if city:
        sql += " AND city=?"
        params.append(city)
    sql += " ORDER BY rating DESC LIMIT ?"
    params.append(limit)
    rows = conn.execute(sql, params).fetchall()
    return [dict(r) for r in rows]


def get_cities_with_attractions() -> list[str]:
    """返回有景点数据的城市列表。"""
    conn = get_conn()
    rows = conn.execute(
        "SELECT DISTINCT city FROM attractions ORDER BY city"
    ).fetchall()
    return [r["city"] for r in rows]


# ---------- 生成日志 ----------

def log_generation(city: str, started_at: str, finished_at: str,
                   cells_total: int, cells_success: int, duration: float,
                   source: str = "scheduled") -> int:
    """记录一次生成任务的统计。"""
    conn = get_conn()
    cur = conn.cursor()
    cur.execute(
        """INSERT INTO generation_log
           (city, started_at, finished_at, cells_total, cells_success, duration_seconds, source)
           VALUES (?,?,?,?,?,?,?)""",
        (city, started_at, finished_at, cells_total, cells_success, duration, source),
    )
    conn.commit()
    return cur.lastrowid


def cleanup_old_logs(days: int = 90) -> int:
    """删除 N 天前的 generation_log 记录。返回删除数量。"""
    conn = get_conn()
    cur = conn.cursor()
    cur.execute(
        "DELETE FROM generation_log WHERE started_at < datetime('now', ?)",
        (f"-{days} days",),
    )
    conn.commit()
    deleted = cur.rowcount
    if deleted > 0:
        print(f"[db] 清理了 {deleted} 条 {days} 天前的 generation_log")
    return deleted


def cleanup_old_cache(days: int = 30) -> int:
    """删除行程已结束超过 N 天的 matrix 缓存。返回删除数量。"""
    conn = get_conn()
    cur = conn.cursor()
    cur.execute(
        "DELETE FROM trip_matrix_cache WHERE end_date < date('now', ?)",
        (f"-{days} days",),
    )
    conn.commit()
    deleted = cur.rowcount
    if deleted > 0:
        print(f"[db] 清理了 {deleted} 条 {days} 天前的 matrix 缓存")
    return deleted


def get_recent_generations(limit: int = 20) -> list[dict]:
    """获取最近的生成记录（管理后台用）。"""
    conn = get_conn()
    conn.row_factory = sqlite3.Row
    rows = conn.execute(
        "SELECT * FROM generation_log ORDER BY started_at DESC LIMIT ?",
        (limit,),
    ).fetchall()
    return [dict(r) for r in rows]


def get_overview_stats() -> dict:
    """系统总览统计（管理后台用）。"""
    conn = get_conn()
    cities_count = conn.execute(
        "SELECT COUNT(DISTINCT city) FROM trip_matrix_cache"
    ).fetchone()[0]
    cells_total = conn.execute("SELECT COUNT(*) FROM trip_matrix_cache").fetchone()[0]
    cells_success = conn.execute(
        "SELECT COUNT(*) FROM trip_matrix_cache WHERE score IS NOT NULL"
    ).fetchone()[0]
    gen_count = conn.execute("SELECT COUNT(*) FROM generation_log").fetchone()[0]
    total_seconds = conn.execute(
        "SELECT COALESCE(SUM(duration_seconds), 0) FROM generation_log"
    ).fetchone()[0]

    return {
        "seed_cities": len(get_seed_cities()),
        "cached_cities": cities_count,
        "cells_total": cells_total,
        "cells_success": cells_success,
        "cache_hit_rate": round(cells_success / cells_total * 100, 1) if cells_total else 0,
        "generation_runs": gen_count,
        "total_compute_seconds": round(total_seconds, 1),
    }


if __name__ == "__main__":
    init_db()
    print(f"DB: {DB_PATH}")
    print(f"济南坐标: {lookup_city('济南')}")
    print(f"朝阳区: {lookup_county('朝阳区')}")
    print(f"杭州西湖区坐标: {lookup_origin_county('浙江省', '杭州市', '西湖区')}")