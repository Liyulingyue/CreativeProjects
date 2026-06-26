# 中国行政区划数据（TravelSites）

为 TravelSites 项目准备的中国行政区划数据，基于
[airyland/china-area-data](https://github.com/airyland/china-area-data) 生成。

## 文件说明

| 文件 | 大小 | 说明 |
| --- | --- | --- |
| `china_regions_raw.json` | 117.8 KB | 原始嵌套字典（省/市/县三层） |
| `china_regions.json`     | 114.8 KB | 简化版，结构更友好，按省名/市名索引 |
| `validation.txt`         | -        | 验证查询结果 |

## 统计

- 省（直辖市/自治区）数：**34**
- 市（地级）数：**374**
- 县（区/县级市）数：**3116**

> 直辖市（北京/天津/上海/重庆）下挂一个与省同名的伪"市"节点，
> 其 `is_municipality` 字段为 `true`，`code` 与省级 `code` 一致。
> 已自动跳过 `市辖区` 这种伪县名条目。

## 数据结构

```json
{
  "<省名>": {
    "code": "<省代码>",
    "cities": {
      "<市名>": {
        "code": "<市代码>",
        "is_municipality": true,   // 仅直辖市节点存在
        "counties": ["<县名1>", "<县名2>", ...]
      }
    }
  }
}
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
    return {
        "province": province,
        "city": city,
        "code": c["code"],
        "county": county,
        "found": county in c["counties"],
    }

print(find_county(regions, "河南省", "洛阳市", "栾川县"))
```

## 数据来源

- 仓库：<https://github.com/airyland/china-area-data>
- 文件：<https://raw.githubusercontent.com/airyland/china-area-data/master/data.json>
- 编码：UTF-8
- 生成时间：2026-06-26 06:53 UTC

生成脚本：见本仓库 `data/build_china_regions.py`（如已归档）。
