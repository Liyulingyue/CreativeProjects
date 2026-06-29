"""
将 data/matrix_cache/*.json 迁移到 SQLite 的 trip_matrix_cache 表。

数据结构转换：
- JSON 文件: matrix_济南.json
  - city: "济南"
  - generated_at: "2026-06-27T..."
  - cells: [{start_offset, duration, start_date, end_date, score, recommendation, weather_summary, full_result}, ...]

- DB 表: trip_matrix_cache
  - city, start_date, duration, end_date,
    score, recommendation, weather_summary,
    full_result (JSON 字符串),
    generated_at
"""
import json
import sqlite3
from pathlib import Path

DATA_DIR = Path(__file__).resolve().parent.parent / "data"
DB_PATH = DATA_DIR / "travelsites.db"
CACHE_DIR = DATA_DIR / "matrix_cache"


def migrate():
    conn = sqlite3.connect(str(DB_PATH))
    cur = conn.cursor()

    cur.execute("DELETE FROM trip_matrix_cache")

    json_files = sorted(CACHE_DIR.glob("matrix_*.json"))
    print(f"Found {len(json_files)} cache files")

    total_cells = 0
    for jf in json_files:
        with open(jf, "r", encoding="utf-8") as f:
            data = json.load(f)

        city = data.get("city")
        generated_at = data.get("generated_at")

        for cell in data.get("cells", []):
            try:
                full = json.dumps(cell.get("full_result"), ensure_ascii=False)
                cur.execute(
                    """INSERT OR REPLACE INTO trip_matrix_cache
                       (city, start_date, duration, end_date,
                        score, recommendation, weather_summary, full_result, generated_at)
                       VALUES (?,?,?,?,?,?,?,?,?)""",
                    (
                        city,
                        cell.get("start_date"),
                        cell.get("duration"),
                        cell.get("end_date"),
                        cell.get("score"),
                        cell.get("recommendation"),
                        cell.get("weather_summary"),
                        full,
                        generated_at,
                    ),
                )
                total_cells += 1
            except Exception as e:
                print(f"  ERROR {city}/{cell}: {e}")

    conn.commit()
    print(f"Migrated {total_cells} cells")

    # 验证
    rows = cur.execute("SELECT COUNT(*), COUNT(DISTINCT city) FROM trip_matrix_cache").fetchone()
    print(f"DB: {rows[0]} cells, {rows[1]} cities")


def query_city_matrix(city: str) -> list[dict]:
    """从 SQLite 查询某城市所有 cell。"""
    conn = sqlite3.connect(str(DB_PATH))
    conn.row_factory = sqlite3.Row
    rows = conn.execute(
        """SELECT * FROM trip_matrix_cache WHERE city=? ORDER BY start_date, duration""",
        (city,),
    ).fetchall()
    return [dict(r) for r in rows]


def query_by_date(start_date: str, end_date: str) -> list[dict]:
    """查询日期范围内的所有 cell（用于 search）。"""
    conn = sqlite3.connect(str(DB_PATH))
    conn.row_factory = sqlite3.Row
    rows = conn.execute(
        """SELECT * FROM trip_matrix_cache WHERE start_date=? AND end_date=?""",
        (start_date, end_date),
    ).fetchall()
    return [dict(r) for r in rows]


if __name__ == "__main__":
    migrate()