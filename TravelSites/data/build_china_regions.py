# -*- coding: utf-8 -*-
"""Build TravelSites/data/china_regions.json from airyland/china-area-data."""
from __future__ import annotations

import json
import sys
from datetime import datetime, timezone
from pathlib import Path

import httpx

DATA_DIR = Path(r"E:\Codes\CreativeProjects\TravelSites\data")
RAW_PATH = DATA_DIR / "china_regions_raw.json"
SIMPLIFIED_PATH = DATA_DIR / "china_regions.json"
README_PATH = DATA_DIR / "README.md"
VALIDATION_PATH = DATA_DIR / "validation.txt"

RAW_URL = (
    "https://raw.githubusercontent.com/airyland/china-area-data/master/data.json"
)

MUNICIPALITY_CODES = {"110000", "120000", "310000", "500000"}  # 京/津/沪/渝
MUNICIPALITY_NAMES = {"北京市", "天津市", "上海市", "重庆市"}


def download_raw() -> dict:
    print(f"[1/4] Downloading {RAW_URL} ...")
    with httpx.Client(timeout=60.0, follow_redirects=True) as client:
        resp = client.get(RAW_URL)
        resp.raise_for_status()
        text = resp.text
    RAW_PATH.write_text(text, encoding="utf-8")
    size_kb = RAW_PATH.stat().st_size / 1024
    print(f"      Saved {RAW_PATH} ({size_kb:.1f} KB)")
    return json.loads(text)


def transform(raw: dict) -> dict:
    print("[2/4] Transforming to simplified structure ...")
    provinces = raw["86"]  # {code: name}

    result: dict = {}
    total_cities = 0
    total_counties = 0
    skipped_municipality = 0
    skipped_shixiaqu = 0

    for prov_code, prov_name in provinces.items():
        prov_entry: dict = {"code": prov_code, "cities": {}}
        cities_raw = raw.get(prov_code, {})

        is_municipality = prov_code in MUNICIPALITY_CODES or prov_name in MUNICIPALITY_NAMES

        if is_municipality:
            # 直辖市的原始结构是：
            #   raw[prov_code] = {pseudo_city_code: "市辖区" | "县", ...}
            #   raw[pseudo_city_code] = {county_code: county_name, ...}
            # 例如 raw["110000"] = {"110100": "市辖区"}, raw["110100"] = 真正的区
            # 重庆还会多一个 500200 = "县" 的桶
            counties: list[str] = []
            for pseudo_city_code, pseudo_city_name in cities_raw.items():
                sub = raw.get(pseudo_city_code, {})
                for county_code, county_name in sub.items():
                    if county_name == "市辖区":
                        skipped_shixiaqu += 1
                        continue
                    counties.append(county_name)
                    total_counties += 1
            prov_entry["cities"][prov_name] = {
                "code": prov_code,
                "counties": counties,
                "is_municipality": True,
            }
            total_cities += 1
            skipped_municipality += 1
            result[prov_name] = prov_entry
            continue

        for city_code, city_name in cities_raw.items():
            counties_raw = raw.get(city_code, {})
            counties = []
            for county_code, county_name in counties_raw.items():
                # 跳过"市辖区"伪县名
                if county_name == "市辖区":
                    skipped_shixiaqu += 1
                    continue
                counties.append(county_name)
                total_counties += 1
            prov_entry["cities"][city_name] = {
                "code": city_code,
                "counties": counties,
            }
            total_cities += 1

        result[prov_name] = prov_entry

    print(f"      Provinces: {len(result)}")
    print(f"      Cities   : {total_cities}")
    print(f"      Counties : {total_counties}")
    print(f"      Skipped municipality-direct entries: {skipped_municipality}")
    print(f"      Skipped '市辖区' pseudo-counties   : {skipped_shixiaqu}")
    return result


def write_simplified(simplified: dict) -> None:
    print(f"[2/4] Writing simplified JSON -> {SIMPLIFIED_PATH}")
    text = json.dumps(simplified, ensure_ascii=False, indent=2, sort_keys=True)
    SIMPLIFIED_PATH.write_text(text, encoding="utf-8")
    size_kb = SIMPLIFIED_PATH.stat().st_size / 1024
    print(f"      Saved ({size_kb:.1f} KB)")


def write_readme(simplified: dict) -> None:
    print("[3/4] Writing README.md ...")
    provinces = len(simplified)
    cities = sum(len(p["cities"]) for p in simplified.values())
    counties = sum(
        len(c["counties"]) for p in simplified.values() for c in p["cities"].values()
    )
    raw_size_kb = RAW_PATH.stat().st_size / 1024
    simp_size_kb = SIMPLIFIED_PATH.stat().st_size / 1024
    now = datetime.now(timezone.utc).strftime("%Y-%m-%d %H:%M UTC")

    readme = f"""# 中国行政区划数据（TravelSites）

为 TravelSites 项目准备的中国行政区划数据，基于
[airyland/china-area-data](https://github.com/airyland/china-area-data) 生成。

## 文件说明

| 文件 | 大小 | 说明 |
| --- | --- | --- |
| `china_regions_raw.json` | {raw_size_kb:.1f} KB | 原始嵌套字典（省/市/县三层） |
| `china_regions.json`     | {simp_size_kb:.1f} KB | 简化版，结构更友好，按省名/市名索引 |
| `validation.txt`         | -        | 验证查询结果 |

## 统计

- 省（直辖市/自治区）数：**{provinces}**
- 市（地级）数：**{cities}**
- 县（区/县级市）数：**{counties}**

> 直辖市（北京/天津/上海/重庆）下挂一个与省同名的伪"市"节点，
> 其 `is_municipality` 字段为 `true`，`code` 与省级 `code` 一致。
> 已自动跳过 `市辖区` 这种伪县名条目。

## 数据结构

```json
{{
  "<省名>": {{
    "code": "<省代码>",
    "cities": {{
      "<市名>": {{
        "code": "<市代码>",
        "is_municipality": true,   // 仅直辖市节点存在
        "counties": ["<县名1>", "<县名2>", ...]
      }}
    }}
  }}
}}
```

## 使用示例

### Python：加载与查询

```python
import json
from pathlib import Path

DATA = Path(__file__).parent / "data" / "china_regions.json"
regions = json.loads(DATA.read_text(encoding="utf-8"))

# 查一个省
henan = regions["河南省"]
print(henan["code"])              # 410000
print(list(henan["cities"].keys()))

# 查一个市 -> 县
luoyang = henan["cities"]["洛阳市"]
print(luoyang["code"])            # 410300
print(luoyang["counties"])        # ['老城区', '西工区', ...]

# 直辖市（伪 city 节点，is_municipality == True）
bj = regions["北京市"]
print(bj["code"])                 # 110000
haidian = bj["cities"]["北京市"]["counties"]
assert "海淀区" in haidian
```

### 工具函数

```python
def find_county(regions, province, city, county):
    prov = regions.get(province)
    if not prov:
        return None
    c = prov["cities"].get(city)
    if not c:
        return None
    return {{
        "province": province,
        "city": city,
        "code": c["code"],
        "county": county,
        "found": county in c["counties"],
    }}

print(find_county(regions, "河南省", "洛阳市", "栾川县"))
```

## 数据来源

- 仓库：<https://github.com/airyland/china-area-data>
- 文件：<https://raw.githubusercontent.com/airyland/china-area-data/master/data.json>
- 编码：UTF-8
- 生成时间：{now}

生成脚本：见本仓库 `data/build_china_regions.py`（如已归档）。
"""
    README_PATH.write_text(readme, encoding="utf-8")


def validate(simplified: dict) -> str:
    print("[4/4] Running validation queries ...")
    lines: list[str] = []
    lines.append("TravelSites / data validation")
    lines.append("=" * 60)

    def query(province: str, city: str, county: str) -> tuple[bool, str]:
        prov = simplified.get(province)
        if not prov:
            return False, f"missing province: {province}"
        c = prov["cities"].get(city)
        if not c:
            return False, f"missing city {city} under {province}"
        if county not in c["counties"]:
            return False, f"missing county {county} under {province}/{city}"
        return True, (
            f"{province}({prov['code']}) -> {city}({c['code']}) -> {county}  OK"
        )

    cases = [
        ("河南省", "洛阳市", "栾川县"),
        ("四川省", "成都市", "都江堰市"),
        ("北京市", "北京市", "海淀区"),
        ("北京市", "北京市", "朝阳区"),
        ("上海市", "上海市", "浦东新区"),
        ("重庆市", "重庆市", "渝中区"),
    ]
    ok = 0
    for prov, city, county in cases:
        success, msg = query(prov, city, county)
        lines.append(("PASS " if success else "FAIL ") + msg)
        if success:
            ok += 1

    lines.append("-" * 60)
    lines.append(f"Total: {ok}/{len(cases)} passed")

    # 额外统计
    lines.append("")
    lines.append("Additional checks:")
    henan_cities = list(simplified["河南省"]["cities"].keys())
    lines.append(f"  河南省 has {len(henan_cities)} cities; sample: {henan_cities[:5]}")
    bj = simplified["北京市"]
    bj_counties = bj["cities"]["北京市"]["counties"]
    lines.append(f"  北京市 counties: {bj_counties}")
    lines.append(f"  海淀区 in 北京 counties: {'海淀区' in bj_counties}")

    text = "\n".join(lines) + "\n"
    VALIDATION_PATH.write_text(text, encoding="utf-8")
    print(text)
    return text


def main() -> int:
    DATA_DIR.mkdir(parents=True, exist_ok=True)
    raw = download_raw()
    simplified = transform(raw)
    write_simplified(simplified)
    write_readme(simplified)
    validate(simplified)
    print("\nDone.")
    return 0


if __name__ == "__main__":
    sys.exit(main())