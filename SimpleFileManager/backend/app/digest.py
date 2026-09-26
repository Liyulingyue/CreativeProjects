"""Knowledge Digest: daily digest of new/changed files, LLM-composed into markdown.

Falls back to a plain listing when the LLM is unavailable, so the digest is
always produced.
"""

import sqlite3
import time
import traceback
from datetime import datetime
from pathlib import Path
from typing import Optional

from .deps import DATA_DIR, get_storage_root, state

MAX_FILES_PER_DIGEST = 40
PREVIEW_CHARS = 400


class DigestService:
    def __init__(self, db_path: str, manifest_db_path: str):
        self.db_path = db_path
        self.manifest_db_path = manifest_db_path
        self._init_db()

    def _get_conn(self) -> sqlite3.Connection:
        return sqlite3.connect(self.db_path, check_same_thread=False)

    def _init_db(self):
        conn = self._get_conn()
        conn.execute("""
            CREATE TABLE IF NOT EXISTS digests (
                date TEXT PRIMARY KEY,
                content TEXT NOT NULL,
                files_count INTEGER DEFAULT 0,
                generated_by TEXT DEFAULT 'llm',
                created_at INTEGER
            )
        """)
        conn.commit()
        conn.close()

    # ---- changed files ----

    def _files_indexed_today(self) -> list[str]:
        today_start = datetime.now().replace(hour=0, minute=0, second=0, microsecond=0).timestamp()
        conn = sqlite3.connect(self.manifest_db_path, check_same_thread=False)
        try:
            rows = conn.execute(
                "SELECT path FROM file_index WHERE indexed_at >= ? ORDER BY indexed_at DESC LIMIT ?",
                (int(today_start), MAX_FILES_PER_DIGEST)
            ).fetchall()
        except sqlite3.OperationalError:
            rows = []
        finally:
            conn.close()
        return [r[0] for r in rows]

    def _collect_previews(self) -> list[dict]:
        items = []
        for path in self._files_indexed_today():
            try:
                with open(path, "r", encoding="utf-8", errors="ignore") as f:
                    preview = f.read(PREVIEW_CHARS)
                stat = Path(path).stat()
                items.append({
                    "path": path,
                    "name": Path(path).name,
                    "size": stat.st_size,
                    "preview": preview,
                })
            except (PermissionError, OSError):
                continue
        return items

    # ---- generation ----

    def _llm_compose(self, date: str, items: list[dict]) -> Optional[str]:
        file_list = "\n".join(
            f"- 【{i['name']}】 {i['path']}\n  内容摘要: {i['preview'][:200]}"
            for i in items
        )
        prompt = f"""以下是 {date} 当天新增或变动的 {len(items)} 个文件的清单与内容摘要。
请为用户生成一份「知识日报」，要求：
1. 用 markdown 格式
2. 开头用 2-3 句话总结今天的数据变动概况
3. 按主题给文件归类（如"文档"、"代码"、"配置"等），每个文件一行，附一句它是什么的说明
4. 结尾给出 1-2 条整理建议

文件清单:
{file_list}"""

        try:
            from .llm_client import chat_completion
            result = chat_completion(
                messages=[
                    {"role": "system", "content": "你是一个私有知识库助手，负责把每天新增的文件整理成简洁实用的知识日报。"},
                    {"role": "user", "content": prompt},
                ],
                temperature=0.5,
            )
            return result.get("content")
        except Exception:
            traceback.print_exc()
            return None

    def _fallback_compose(self, date: str, items: list[dict]) -> str:
        lines = [f"# 知识日报 · {date}", "", f"今日共 {len(items)} 个文件新增或变动（LLM 摘要生成失败，以下为原始清单）：", ""]
        for i in items:
            lines.append(f"- **{i['name']}** — `{i['path']}` ({i['size']} bytes)")
        return "\n".join(lines)

    def generate(self, date: Optional[str] = None) -> dict:
        date = date or datetime.now().strftime("%Y-%m-%d")
        items = self._collect_previews()
        if not items:
            return {"success": False, "message": "今天没有新增或变动的文件，先运行自动索引再生成日报。"}

        content = self._llm_compose(date, items)
        generated_by = "llm"
        if not content:
            content = self._fallback_compose(date, items)
            generated_by = "fallback"

        conn = self._get_conn()
        conn.execute(
            "INSERT OR REPLACE INTO digests (date, content, files_count, generated_by, created_at) VALUES (?, ?, ?, ?, ?)",
            (date, content, len(items), generated_by, int(time.time() * 1000))
        )
        conn.commit()
        conn.close()
        return {"success": True, "date": date, "files_count": len(items), "generated_by": generated_by}

    # ---- queries ----

    def get(self, date: str) -> Optional[dict]:
        conn = self._get_conn()
        row = conn.execute(
            "SELECT date, content, files_count, generated_by, created_at FROM digests WHERE date = ?",
            (date,)
        ).fetchone()
        conn.close()
        if not row:
            return None
        return {"date": row[0], "content": row[1], "files_count": row[2], "generated_by": row[3], "created_at": row[4]}

    def latest(self) -> Optional[dict]:
        conn = self._get_conn()
        row = conn.execute(
            "SELECT date FROM digests ORDER BY date DESC LIMIT 1"
        ).fetchone()
        conn.close()
        return self.get(row[0]) if row else None

    def list_digests(self, limit: int = 30) -> list[dict]:
        conn = self._get_conn()
        rows = conn.execute(
            "SELECT date, files_count, generated_by, created_at FROM digests ORDER BY date DESC LIMIT ?",
            (limit,)
        ).fetchall()
        conn.close()
        return [{"date": r[0], "files_count": r[1], "generated_by": r[2], "created_at": r[3]} for r in rows]


digest_service = DigestService(
    db_path=str(DATA_DIR / "digest.db"),
    manifest_db_path=str(DATA_DIR / "file_index.db"),
)
