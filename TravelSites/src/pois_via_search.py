"""
基于搜索引擎（bing）+ LLM 实体抽取的实时景点补全。

免费、无 key。用户搜某城市时触发，缓存到 DB。

策略链：
  1. DB 已有 → 直接返回
  2. Bing 搜索 "城市 著名景点" → 取前 5 个结果的 snippet
  3. LLM 提炼景点名（OpenAIJsonWrapper 强制 JSON 输出）
  4. 验证去重 → 写回 DB
"""
import json
import re
import sqlite3
from pathlib import Path
from typing import Optional

import httpx

DATA_DIR = Path(__file__).resolve().parent.parent / "data"
DB_PATH = DATA_DIR / "travelsites.db"

SEARCH_ENGINES = [
    "https://cn.bing.com/search",
    "https://www.bing.com/search",
]


def search_bing(query: str, limit: int = 5) -> list[dict]:
    """Bing 搜索。返回 [{title, snippet}] 列表。"""
    for url in SEARCH_ENGINES:
        try:
            with httpx.Client(timeout=15.0, follow_redirects=True) as client:
                resp = client.get(
                    url,
                    params={"q": query},
                    headers={
                        "User-Agent": "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/119 Safari/537.36",
                        "Accept-Language": "zh-CN,zh;q=0.9",
                    },
                )
                if resp.status_code != 200:
                    continue
                return _parse_bing(resp.text, limit)
        except Exception:
            continue
    return []


def _parse_bing(html: str, limit: int) -> list[dict]:
    """解析 Bing HTML。"""
    results = []
    titles_raw = re.findall(r'<h2[^>]*>\s*<a[^>]*>(.*?)</a>\s*</h2>', html, re.DOTALL)
    titles = [re.sub(r'<[^>]+>', '', t).strip() for t in titles_raw]

    segs = re.split(r'<h2[^>]*>.*?</h2>', html, flags=re.DOTALL)
    snippets = []
    for seg in segs[1:]:
        m = re.search(r'<p[^>]*>(.*?)</p>', seg, re.DOTALL)
        if m:
            text = re.sub(r'<[^>]+>', '', m.group(1)).strip()
            if 20 < len(text) < 500:
                snippets.append(text)
        if len(snippets) >= limit:
            break

    for i, title in enumerate(titles[:limit]):
        snippet = snippets[i] if i < len(snippets) else ""
        if title:
            results.append({"title": title, "snippet": snippet})
    return results


def extract_pois_via_llm(city: str, search_results: list[dict]) -> list[str]:
    """LLM 提取景点。用 OpenAIJsonWrapper 强制 JSON 输出，跨模型可靠。"""
    if not search_results:
        return []

    text = "\n".join(
        f"- {r['title']}: {r['snippet']}"
        for r in search_results
    )

    from src.config import API_KEY, BASE_URL, MODEL_NAME
    from openai import OpenAI
    from openaijsonwrapper import OpenAIJsonWrapper

    client = OpenAI(api_key=API_KEY, base_url=BASE_URL)

    target_structure = {
        "attractions": ["故宫", "颐和园"],
    }
    background = f"提取{city}最具代表性的 5-8 个真实著名景点"
    requirements = [
        "必须是真实存在的著名景点，禁止编造",
        "去掉通用后缀（如'故宫博物院'→'故宫'）",
        "只输出景点名，每行一个纯净字符串",
    ]

    prompt = f"提取【{city}】最知名的 5-8 个旅游景点的名字。\n\n搜索结果：\n{text[:2000]}"

    wrapper = OpenAIJsonWrapper(
        client, model=MODEL_NAME,
        target_structure=target_structure,
        background=background,
        requirements=requirements,
    )
    try:
        response = wrapper.chat(messages=[{"role": "user", "content": prompt}])
        if response.get("error"):
            print(f"  [poi] wrapper 失败: {response['error']}")
            return []
        data = response.get("data") or {}
        names = data.get("attractions", [])
        if isinstance(names, str):
            names = [n.strip() for n in names.split("\n") if n.strip()]
        return [str(n).strip() for n in names if n][:8]
    except Exception as e:
        print(f"  [poi] LLM 提取失败: {e}")
        return []


def extract_pois(city: str, search_results: list[dict]) -> list[str]:
    """统一入口：LLM 提取景点名。"""
    return extract_pois_via_llm(city, search_results)


def get_db_pois(city: str, limit: int = 30) -> list[dict]:
    """从 DB 查景点。"""
    conn = sqlite3.connect(str(DB_PATH))
    conn.row_factory = sqlite3.Row
    rows = conn.execute(
        "SELECT name, category, lat, lon, source FROM attractions WHERE city=? ORDER BY rating DESC NULLS LAST LIMIT ?",
        (city, limit),
    ).fetchall()
    return [dict(r) for r in rows]


def insert_poi(city: str, name: str) -> bool:
    """写入 DB（如果不存在）。"""
    conn = sqlite3.connect(str(DB_PATH))
    cur = conn.cursor()
    try:
        cur.execute(
            """INSERT OR IGNORE INTO attractions
               (name, city, source, verified, created_at)
               VALUES (?, ?, 'search_runtime', 0, datetime('now'))""",
            (name, city),
        )
        ok = cur.rowcount > 0
        conn.commit()
        return ok
    finally:
        conn.close()


def fetch_pois_for_city(city: str) -> list[dict]:
    """完整链路：DB → search_runtime 缓存。"""
    pois = get_db_pois(city, limit=30)
    if len(pois) >= 5:
        return pois

    results = search_bing(f"{city} 著名景点", limit=5)
    if not results:
        return pois

    names = extract_pois(city, results)
    if not names:
        return pois

    for name in names:
        insert_poi(city, name)

    return get_db_pois(city, limit=30)


if __name__ == "__main__":
    import sys
    sys.path.insert(0, str(Path(__file__).resolve().parent.parent))
    for city in ["北京", "上海", "拉萨", "三亚", "桂林"]:
        print(f"\n=== {city} ===")
        results = search_bing(f"{city} 著名景点", limit=5)
        print(f"搜索: {len(results)} 条")
        for r in results[:3]:
            print(f"  - {r['title'][:60]}")
        names = extract_pois(city, results)
        print(f"提取: {names}")
