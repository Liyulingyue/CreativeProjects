import json
import csv
from pathlib import Path
from typing import Optional
from .planner import TripPlanResult
from .matrix import MatrixCell


REC_SHORT: dict[str, str] = {
    "强烈推荐": "强推",
    "推荐": "推荐",
    "勉强可行": "勉强",
    "建议改期": "改期",
}


def _rec_short(rec: Optional[str]) -> str:
    if not rec:
        return "?"
    return REC_SHORT.get(rec, rec)


def export_to_json(result: TripPlanResult, output_path: str | Path) -> None:
    output_path = Path(output_path)
    with open(output_path, "w", encoding="utf-8") as f:
        json.dump(result.to_dict(), f, indent=2, ensure_ascii=False)


def _bar(score: int) -> str:
    if score is None:
        return "N/A"
    n_filled = max(0, min(20, score // 5))
    return "#" * n_filled + "-" * (20 - n_filled)


def print_summary(result: TripPlanResult) -> None:
    print(f"\n{'='*60}")
    print(f"规划: {result.city}  {result.start_date} ~ {result.end_date}  ({result.duration_days} 天)")
    print(f"{'='*60}")

    if result.weather_forecast:
        print(f"\n【真实天气】")
        for w in result.weather_forecast:
            prob = w.get("precipitation_probability")
            prob_str = f"{prob}%" if prob is not None else "N/A"
            print(f"  {w['date']}  {w['weather_desc']:<10}  {w['temp_min']:.0f}°C~{w['temp_max']:.0f}°C  "
                  f"降水 {w['precipitation_mm']:.1f}mm  概率 {prob_str}")

    if not result.success:
        print(f"\n[失败] {result.error}")
        if result.raw_content:
            print(f"\n原始返回:\n{result.raw_content[:500]}")
        return

    data = result.data
    print(f"\n省份: {data.get('province', 'N/A')}")
    print(f"最佳季节: {data.get('best_season', 'N/A')}")
    print(f"城市标签: {', '.join(data.get('city_tags', []))}")

    score = data.get("score")
    recommendation = data.get("recommendation", "N/A")
    if score is not None:
        print(f"\n【整体评分】 {score}/100   {recommendation}")
        print(f"  {_bar(score)}")
        breakdown = data.get("score_breakdown", {})
        for k in ("days_match", "weather", "attraction_density", "transport"):
            v = breakdown.get(k)
            if v is not None:
                label = {"days_match": "天数匹配", "weather": "天气友好",
                         "attraction_density": "景点丰富", "transport": "交通便利"}.get(k, k)
                print(f"    {label:<8}  {v:>3}/100  {_bar(v)}")

    if data.get("weather_strategy"):
        print(f"\n【天气策略】\n  {data['weather_strategy']}")

    print(f"\n简介: {data.get('city_intro', 'N/A')}")

    attractions = data.get("attractions", [])
    nearby = [a for a in attractions if a.get("is_nearby")]
    print(f"\n景点: {len(attractions)} 个  |  近郊: {len(nearby)} 个")
    for a in attractions:
        marker = "★近郊" if a.get("is_nearby") else "  市区"
        print(f"  {marker}  {a.get('name')} ({a.get('category')})  - {a.get('location')}  {a.get('distance_from_center_km')}km")

    daily_plan = data.get("daily_plan", [])
    print(f"\n每日行程:")
    for day in daily_plan:
        print(f"\n  Day {day.get('day')}  {day.get('date')}  主题: {day.get('theme')}")
        print(f"    天气: {day.get('weather_hint')}")
        for route in day.get("routes", []):
            print(f"    [{route.get('route_id')}] {', '.join(route.get('tags', []))}  {route.get('total_hours')}h")
            for act in route.get("activities", []):
                print(f"      - {act.get('time_slot')}  {act.get('attraction')}  ({act.get('hours')}h)  {act.get('notes')}")

    foods = data.get("food_recommendations", [])
    if foods:
        print(f"\n美食:")
        for f in foods:
            print(f"  - {f.get('name')} ({f.get('type')})  {f.get('location')}  招牌: {f.get('signature')}  {f.get('price_range')}")

    if data.get("transportation_tips"):
        print(f"\n交通: {data['transportation_tips']}")
    if data.get("accommodation_tips"):
        print(f"\n住宿: {data['accommodation_tips']}")

    tips = data.get("general_tips", [])
    if tips:
        print(f"\n贴士:")
        for t in tips:
            print(f"  - {t}")


def print_matrix(cells: list[MatrixCell]) -> None:
    if not cells:
        print("(无数据)")
        return

    by: dict[tuple[int, int], MatrixCell] = {(c.start_offset, c.duration): c for c in cells}
    offsets = sorted({c.start_offset for c in cells})
    durations = sorted({c.duration for c in cells})

    success = sum(1 for c in cells if c.success)
    print(f"\n成功: {success}/{len(cells)}")

    header = f"{'出发日':<8}"
    for d in durations:
        header += f"  {str(d) + '天':<14}"
    print(f"\n{header}")
    print("-" * len(header.encode("gbk", errors="ignore")))

    for off in offsets:
        row = f"+{off}天后  "
        for dur in durations:
            c = by.get((off, dur))
            if c is None:
                row += f"  {'-':<14}"
            elif not c.success:
                row += f"  {'ERR':<14}"
            elif c.score is not None:
                row += f"  {c.score}/{_rec_short(c.recommendation):<10}"
            else:
                row += f"  {'?':<14}"
        print(row)

    print()
    print("推荐度分布:")
    rec_counts: dict[str, int] = {}
    for c in cells:
        if c.success and c.recommendation:
            rec_counts[c.recommendation] = rec_counts.get(c.recommendation, 0) + 1
    for rec, cnt in sorted(rec_counts.items(), key=lambda x: -x[1]):
        print(f"  {rec:<8}  {cnt} 格")

    print()
    print("高分 Top 5:")
    top = sorted(
        [c for c in cells if c.success and c.score is not None],
        key=lambda c: -c.score,
    )[:5]
    for c in top:
        ws = c.weather_summary or ""
        if len(ws) > 60:
            ws = ws[:60] + "..."
        print(f"  {c.start_date} ({c.duration}天)  Score {c.score}  {_rec_short(c.recommendation)}")
        print(f"    天气: {ws}")

    print()
    print("低分 Bottom 5:")
    bottom = sorted(
        [c for c in cells if c.success and c.score is not None],
        key=lambda c: c.score,
    )[:5]
    for c in bottom:
        ws = c.weather_summary or ""
        if len(ws) > 60:
            ws = ws[:60] + "..."
        print(f"  {c.start_date} ({c.duration}天)  Score {c.score}  {_rec_short(c.recommendation)}")
        print(f"    天气: {ws}")


def export_matrix_to_csv(cells: list[MatrixCell], output_path: str | Path) -> None:
    output_path = Path(output_path)
    with open(output_path, "w", newline="", encoding="utf-8-sig") as f:
        writer = csv.DictWriter(f, fieldnames=[
            "city", "start_offset", "duration", "start_date", "end_date",
            "score", "recommendation", "weather_summary", "success", "error",
        ])
        writer.writeheader()
        for c in cells:
            city = c.full_result.get("city") if c.full_result else None
            writer.writerow({
                "city": city,
                "start_offset": c.start_offset,
                "duration": c.duration,
                "start_date": c.start_date,
                "end_date": c.end_date,
                "score": c.score,
                "recommendation": c.recommendation,
                "weather_summary": c.weather_summary,
                "success": c.success,
                "error": c.error,
            })


def export_matrix_to_json(cells: list[MatrixCell], output_path: str | Path) -> None:
    output_path = Path(output_path)
    data = [c.to_dict() for c in cells]
    with open(output_path, "w", encoding="utf-8") as f:
        json.dump(data, f, indent=2, ensure_ascii=False)
