"""
2025-2027 中国法定节假日 + 调休数据。

数据来源：国务院办公厅每年发布的节假日通知。

字段说明：
- type=public: 法定节假日
- type=observed: 调休补班（要上班）
- type=weekend: 普通周末
- type=makeup: 调休休假（连休）
- impact_level: 0=正常, 1=小长假, 2=大长假, 3=春节/国庆
"""
import json
import sqlite3
from pathlib import Path
from datetime import date, timedelta
from typing import Optional

DATA_DIR = Path(__file__).resolve().parent.parent / "data"
DB_PATH = DATA_DIR / "travelsites.db"


# 国务院办公厅发布的节假日安排
HOLIDAYS = {
    # 2025 年
    "2025-01-01": ("元旦", "public", 1),
    "2025-01-28": ("春节", "makeup", 3),  # 调休
    "2025-01-29": ("春节", "makeup", 3),
    "2025-01-30": ("春节", "makeup", 3),
    "2025-01-31": ("除夕", "public", 3),
    "2025-02-01": ("春节", "public", 3),
    "2025-02-02": ("春节", "public", 3),
    "2025-02-03": ("春节", "public", 3),
    "2025-02-04": ("春节", "public", 3),
    "2025-02-08": ("春节", "observed", 0),  # 补班
    "2025-04-04": ("清明节", "public", 1),
    "2025-04-06": ("清明节", "observed", 0),
    "2025-05-01": ("劳动节", "public", 1),
    "2025-05-02": ("劳动节", "public", 1),
    "2025-05-05": ("劳动节", "makeup", 1),
    "2025-05-31": ("端午节", "public", 1),
    "2025-06-02": ("端午节", "makeup", 1),
    "2025-10-01": ("国庆节", "public", 2),
    "2025-10-02": ("国庆节", "public", 2),
    "2025-10-03": ("国庆节", "public", 2),
    "2025-10-04": ("中秋节", "public", 2),
    "2025-10-05": ("国庆节", "public", 2),
    "2025-10-06": ("国庆节", "public", 2),
    "2025-10-07": ("国庆节", "public", 2),
    "2025-10-08": ("国庆节", "makeup", 2),
    "2025-10-11": ("国庆节", "observed", 0),

    # 2026 年
    "2026-01-01": ("元旦", "public", 1),
    "2026-01-02": ("元旦", "public", 1),
    "2026-01-03": ("元旦", "makeup", 1),
    "2026-02-16": ("除夕", "public", 3),
    "2026-02-17": ("春节", "public", 3),
    "2026-02-18": ("春节", "public", 3),
    "2026-02-19": ("春节", "public", 3),
    "2026-02-20": ("春节", "public", 3),
    "2026-02-21": ("春节", "makeup", 3),
    "2026-02-22": ("春节", "makeup", 3),
    "2026-02-28": ("春节", "observed", 0),
    "2026-04-04": ("清明节", "public", 1),
    "2026-04-06": ("清明节", "makeup", 1),
    "2026-05-01": ("劳动节", "public", 1),
    "2026-05-02": ("劳动节", "public", 1),
    "2026-05-03": ("劳动节", "makeup", 1),
    "2026-06-19": ("端午节", "public", 1),
    "2026-06-21": ("端午节", "makeup", 1),
    "2026-09-25": ("中秋节", "public", 1),
    "2026-09-27": ("中秋节", "makeup", 1),
    "2026-10-01": ("国庆节", "public", 2),
    "2026-10-02": ("国庆节", "public", 2),
    "2026-10-03": ("国庆节", "public", 2),
    "2026-10-04": ("国庆节", "public", 2),
    "2026-10-05": ("国庆节", "public", 2),
    "2026-10-06": ("国庆节", "public", 2),
    "2026-10-07": ("国庆节", "public", 2),
    "2026-10-08": ("国庆节", "makeup", 2),
    "2026-10-10": ("国庆节", "observed", 0),

    # 2027 年
    "2027-01-01": ("元旦", "public", 1),
    "2027-01-03": ("元旦", "makeup", 1),
    "2027-02-05": ("除夕", "public", 3),
    "2027-02-06": ("春节", "public", 3),
    "2027-02-07": ("春节", "public", 3),
    "2027-02-08": ("春节", "public", 3),
    "2027-02-09": ("春节", "public", 3),
    "2027-02-13": ("春节", "makeup", 3),
    "2027-04-05": ("清明节", "public", 1),
    "2027-05-01": ("劳动节", "public", 1),
    "2027-05-02": ("劳动节", "makeup", 1),
    "2027-06-09": ("端午节", "public", 1),
    "2027-10-01": ("国庆节", "public", 2),
    "2027-10-02": ("国庆节", "public", 2),
    "2027-10-03": ("国庆节", "public", 2),
    "2027-10-04": ("国庆节", "public", 2),
    "2027-10-05": ("中秋节", "public", 2),
    "2027-10-06": ("国庆节", "public", 2),
    "2027-10-07": ("国庆节", "public", 2),
}


def fill_holidays():
    """把节假日数据写入 SQLite（可重复执行）。"""
    conn = sqlite3.connect(str(DB_PATH))
    cur = conn.cursor()

    cur.execute("DELETE FROM holiday_calendar WHERE region_code IS NULL")

    cur.executemany(
        """INSERT OR IGNORE INTO holiday_calendar
           (date, name, type, impact_level, region_code, demographic, source)
           VALUES (?,?,?,?,NULL,NULL,'state_council')""",
        [(d, *info) for d, info in HOLIDAYS.items()],
    )

    conn.commit()

    total = cur.execute("SELECT COUNT(*) FROM holiday_calendar").fetchone()[0]
    print(f"[holidays] {total} 条记录已写入")


def get_holiday_impact(date_str: str) -> tuple[str, int]:
    """
    返回某日期的节假日信息（同步供 score 计算使用）。
    Args:
        date_str: YYYY-MM-DD
    Returns:
        (name, impact_level)，普通日返回 ("", 0)
    """
    conn = sqlite3.connect(str(DB_PATH))
    conn.row_factory = sqlite3.Row
    row = conn.execute(
        """SELECT name, impact_level FROM holiday_calendar
           WHERE date=? AND region_code IS NULL""",
        (date_str,),
    ).fetchone()
    if row:
        return (row["name"], row["impact_level"])
    return ("", 0)


def list_holidays_in_range(start_date: str, end_date: str, region: Optional[str] = None) -> list[dict]:
    """
    返回某日期范围内所有节假日。
    Args:
        start_date: YYYY-MM-DD
        end_date: YYYY-MM-DD
        region: 区域代码（None=全国通用, "HK"=香港等）
    Returns:
        [{"date": "2026-10-01", "name": "国庆节", "type": "public", "impact_level": 2}, ...]
    """
    conn = sqlite3.connect(str(DB_PATH))
    conn.row_factory = sqlite3.Row
    rows = conn.execute(
        """SELECT date, name, type, impact_level, region_code, source FROM holiday_calendar
           WHERE date BETWEEN ? AND ?
           AND (region_code IS NULL OR region_code = ?)
           ORDER BY date""",
        (start_date, end_date, region),
    ).fetchall()
    return [dict(r) for r in rows]


def calculate_holiday_insights(start_date: str, end_date: str) -> dict:
    """
    分析节假日对人/活动/季节的影响（不修改推荐评分，只做附加洞察）。

    评估维度：
    - crowd_level: 人流密度（"low"/"medium"/"high"/"extreme"）
    - activity_level: 活动丰富度（"low"/"medium"/"high"）
    - price_level: 价格上浮（1.0=无影响, 1.5=旺季上浮50%）
    - tips: 出行提示文案
    - 在行程内的节假日名称列表
    """
    holidays = list_holidays_in_range(start_date, end_date)
    if not holidays:
        return {
            "holidays": [],
            "crowd_level": "low",
            "activity_level": "normal",
            "price_multiplier": 1.0,
            "tips": ["平季出行，体验舒适"],
        }

    crowd = "low"
    activity = "normal"
    price_mul = 1.0
    tips = []
    seen_tips = set()
    biggest_impact = 0

    for h in holidays:
        impact = h["impact_level"]
        h_type = h["type"]
        name = h["name"]

        if h_type == "observed":
            # 调休上班
            key = "调休"
            if key not in seen_tips:
                tips.append(f"{h['date']} {name}调休上班")
                seen_tips.add(key)
            continue

        biggest_impact = max(biggest_impact, impact)

        if impact == 3:  # 春节/国庆 7 天大假
            crowd = "extreme"
            activity = "high"
            price_mul = max(price_mul, 1.8)
            key = "大假"
            if key not in seen_tips:
                tips.append(f"{name}期间人流密集，建议提前 1-2 周预订门票/酒店")
                tips.append("热门景点排队可能超过 2 小时，建议清晨或傍晚前往")
                seen_tips.add(key)
        elif impact == 2:  # 国庆/中秋 3 天小长
            crowd = "high"
            activity = "high"
            price_mul = max(price_mul, 1.4)
            key = "小长"
            if key not in seen_tips:
                tips.append(f"{name}小长假出行较多，景点人流偏高，住宿价格上浮")
                seen_tips.add(key)
        elif impact == 1:  # 清明/端午/劳动节 1-3 天
            crowd = "medium"
            activity = "medium"
            price_mul = max(price_mul, 1.15)
            key = "小假"
            if key not in seen_tips:
                tips.append(f"{name}有节庆活动，可体验当地特色文化")
                seen_tips.add(key)
        elif h_type == "weekend":
            crowd = "medium" if crowd == "low" else crowd

    return {
        "holidays": holidays,
        "crowd_level": crowd,
        "activity_level": activity,
        "price_multiplier": round(price_mul, 2),
        "tips": tips,
    }


if __name__ == "__main__":
    fill_holidays()