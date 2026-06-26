"""
为 china_regions.json 渐进式补充经纬度。

设计原则：
- 真相源是 china_regions_enriched.json（不存在则从 china_regions.json 复制）
- 每次只查"缺 lat/lon"的城市，已有的直接跳过
- 多次运行是幂等的
- 中断无副作用，下次接着来

回退链：本地 24 城市 → Open-Meteo → 简化名 → Nominatim → 留空
"""
import asyncio
import json
import re
import sys
import time
from pathlib import Path
from typing import Optional

import httpx

ROOT = Path(__file__).resolve().parent.parent
DATA = ROOT / "data"
SOURCE_FILE = DATA / "china_regions.json"
ENRICHED_FILE = DATA / "china_regions_enriched.json"

LOCAL_COORDS: dict[str, tuple[float, float]] = {
    "北京": (39.9042, 116.4074), "上海": (31.2304, 121.4737),
    "广州": (23.1291, 113.2644), "深圳": (22.5431, 114.0579),
    "杭州": (30.2741, 120.1551), "成都": (30.5728, 104.0668),
    "西安": (34.3416, 108.9398), "南京": (32.0603, 118.7969),
    "洛阳": (34.6197, 112.4539), "苏州": (31.2989, 120.5853),
    "青岛": (36.0671, 120.3826), "厦门": (24.4798, 118.0894),
    "大理": (25.6065, 100.2675), "丽江": (26.8721, 100.2330),
    "黄山": (29.7148, 118.3375), "桂林": (25.2736, 110.2907),
    "长沙": (28.2282, 112.9388), "重庆": (29.5630, 106.5516),
    "武汉": (30.5928, 114.3055), "天津": (39.3434, 117.3616),
    "哈尔滨": (45.8038, 126.5350), "三亚": (18.2528, 109.5119),
    "拉萨": (29.6520, 91.1721), "敦煌": (40.1421, 94.6612),
}

PROV_OVERRIDE: dict[str, tuple[float, float]] = {
    "北京市": (39.9042, 116.4074),
    "上海市": (31.2304, 121.4737),
    "天津市": (39.3434, 117.3616),
    "重庆市": (29.5630, 106.5516),
    "云南省": (25.0389, 102.7183),
    "内蒙古自治区": (40.8426, 111.7491),
    "新疆维吾尔自治区": (43.8256, 87.6168),
    "西藏自治区": (29.6520, 91.1721),
    "宁夏回族自治区": (38.4872, 106.2309),
    "广西壮族自治区": (22.8170, 108.3669),
    "香港特别行政区": (22.3193, 114.1694),
    "澳门特别行政区": (22.1987, 113.5439),
    "台湾省": (25.0330, 121.5654),
}

SUFFIX_PATTERN = re.compile(r"(自治州|自治县|盟|地区|矿区)$")
CONCURRENCY = 5
DELAY_BETWEEN = 0.2


def simplify(name: str) -> str:
    s = SUFFIX_PATTERN.sub("", name)
    return s if s else name


async def geocode_openmeteo(client: httpx.AsyncClient, name: str) -> Optional[tuple[float, float]]:
    try:
        resp = await client.get(
            "https://geocoding-api.open-meteo.com/v1/search",
            params={"name": name, "count": 1, "language": "zh"},
            timeout=15.0,
        )
        if resp.status_code == 200:
            data = resp.json()
            results = data.get("results", [])
            if results:
                return float(results[0]["latitude"]), float(results[0]["longitude"])
    except Exception:
        pass
    return None


async def geocode_nominatim(client: httpx.AsyncClient, name: str) -> Optional[tuple[float, float]]:
    try:
        resp = await client.get(
            "https://nominatim.openstreetmap.org/search",
            params={"q": name, "format": "json", "limit": 1, "countrycodes": "cn"},
            headers={"User-Agent": "TravelSites/0.1"},
            timeout=15.0,
        )
        if resp.status_code == 200:
            results = resp.json()
            if results:
                return float(results[0]["lat"]), float(results[0]["lon"])
    except Exception:
        pass
    return None


async def geocode_with_fallback(client: httpx.AsyncClient, name: str, sem: asyncio.Semaphore) -> tuple[Optional[tuple[float, float]], str]:
    async with sem:
        await asyncio.sleep(DELAY_BETWEEN)
        r = await geocode_openmeteo(client, name)
        if r:
            return r, "openmeteo"
        simplified = simplify(name)
        if simplified != name:
            r = await geocode_openmeteo(client, simplified)
            if r:
                return r, "openmeteo(simplified)"
        r = await geocode_nominatim(client, name)
        if r:
            return r, "nominatim"
        return None, "failed"


def load_or_seed() -> tuple[dict, bool]:
    if ENRICHED_FILE.exists():
        data = json.loads(ENRICHED_FILE.read_text(encoding="utf-8"))
        return data, True
    if not SOURCE_FILE.exists():
        raise FileNotFoundError(f"找不到源文件: {SOURCE_FILE}")
    data = json.loads(SOURCE_FILE.read_text(encoding="utf-8"))
    return data, False


def save(data: dict) -> None:
    ENRICHED_FILE.write_text(
        json.dumps(data, ensure_ascii=False, indent=2),
        encoding="utf-8",
    )


def collect_missing(data: dict) -> list[tuple[str, str, dict]]:
    missing = []
    for prov_name, prov_data in data.items():
        for city_name, city_data in prov_data.get("cities", {}).items():
            if "latitude" not in city_data and "longitude" not in city_data:
                if city_name in LOCAL_COORDS:
                    lat, lon = LOCAL_COORDS[city_name]
                    city_data["latitude"] = round(lat, 4)
                    city_data["longitude"] = round(lon, 4)
                else:
                    missing.append((prov_name, city_name, city_data))
    return missing


def fill_provinces(data: dict) -> None:
    for prov_name, prov_data in data.items():
        if prov_name in PROV_OVERRIDE:
            lat, lon = PROV_OVERRIDE[prov_name]
            prov_data["latitude"] = lat
            prov_data["longitude"] = lon
            prov_data["representative_city"] = prov_name
            continue

        if "latitude" in prov_data:
            continue

        for city_name, city_data in prov_data.get("cities", {}).items():
            if "latitude" in city_data:
                prov_data["latitude"] = city_data["latitude"]
                prov_data["longitude"] = city_data["longitude"]
                prov_data["representative_city"] = city_name
                break


async def main_async() -> int:
    print(f"加载 {'已有' if ENRICHED_FILE.exists() else '源'} JSON...")
    data, was_enriched = load_or_seed()
    if not was_enriched:
        save(data)
        print(f"  从源文件复制到 {ENRICHED_FILE.name}")

    total_cities = sum(len(p.get("cities", {})) for p in data.values())
    filled_cities = sum(
        1
        for p in data.values()
        for c in p.get("cities", {}).values()
        if "latitude" in c
    )
    print(f"  总城市: {total_cities}, 已有坐标: {filled_cities}, 缺: {total_cities - filled_cities}\n")

    missing = collect_missing(data)
    if missing:
        save(data)
        print(f"需查询 {len(missing)} 个城市 (并发 {CONCURRENCY}, 间隔 {DELAY_BETWEEN}s)\n")

        sem = asyncio.Semaphore(CONCURRENCY)
        t0 = time.time()
        done = 0
        ok = 0
        by_source: dict[str, int] = {}

        async with httpx.AsyncClient() as client:
            async def run(prov: str, city: str, city_data: dict) -> None:
                nonlocal done, ok
                coords, source = await geocode_with_fallback(client, city, sem)
                done += 1
                if coords:
                    city_data["latitude"] = round(coords[0], 4)
                    city_data["longitude"] = round(coords[1], 4)
                    ok += 1
                    by_source[source] = by_source.get(source, 0) + 1
                if done % 10 == 0 or done == len(missing):
                    elapsed = time.time() - t0
                    rate = done / elapsed if elapsed > 0 else 0
                    print(f"  [{done}/{len(missing)}] OK={ok}  {elapsed:.0f}s  ({rate:.2f} req/s)", flush=True)
                    fill_provinces(data)
                    save(data)

            await asyncio.gather(*[run(p, c, cd) for p, c, cd in missing])
            fill_provinces(data)
            save(data)

        elapsed = time.time() - t0
        print(f"\n查询耗时: {elapsed:.1f}s")
        print(f"成功: {ok}/{len(missing)}, 失败: {len(missing) - ok}")
        for src, n in sorted(by_source.items()):
            print(f"  {src}: {n}")
    else:
        print("无缺失，跳过查询")

    print()
    total_cities = sum(len(p.get("cities", {})) for p in data.values())
    filled_cities = sum(
        1
        for p in data.values()
        for c in p.get("cities", {}).values()
        if "latitude" in c
    )
    print(f"=== 状态 ===")
    print(f"输出: {ENRICHED_FILE.name} ({ENRICHED_FILE.stat().st_size / 1024:.1f} KB)")
    print(f"已填充: {filled_cities}/{total_cities} ({filled_cities/total_cities*100:.1f}%)")

    failed = [
        f"{p}::{c}"
        for p, pd in data.items()
        for c, cd in pd.get("cities", {}).items()
        if "latitude" not in cd
    ]
    if failed:
        print(f"仍缺 {len(failed)}:")
        for k in failed[:10]:
            print(f"  {k}")
        if len(failed) > 10:
            print(f"  ... (+{len(failed) - 10} more)")
        print(f"\n提示: 再跑一次脚本会跳过已填充的，只查这 {len(failed)} 个")
    return 0


if __name__ == "__main__":
    sys.exit(asyncio.run(main_async()))
