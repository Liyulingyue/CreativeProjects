from datetime import datetime
from typing import Optional

from fastapi import APIRouter, Depends, HTTPException

from ..digest import digest_service
from .auth import require_admin

digest = APIRouter(prefix="/api/digest", tags=["digest"])


@digest.post("/generate")
def generate_digest(date: Optional[str] = None, _admin=Depends(require_admin)) -> dict:
    if date:
        try:
            datetime.strptime(date, "%Y-%m-%d")
        except ValueError:
            raise HTTPException(status_code=400, detail="日期要写成 YYYY-MM-DD")
    return digest_service.generate(date)


@digest.get("/latest")
def latest_digest(_admin=Depends(require_admin)) -> dict:
    d = digest_service.latest()
    return d or {"date": None, "content": None, "files_count": 0}


@digest.get("/list")
def list_digests(limit: int = 30, _admin=Depends(require_admin)) -> dict:
    return {"digests": digest_service.list_digests(limit)}


@digest.get("/{date}")
def get_digest(date: str, _admin=Depends(require_admin)) -> dict:
    d = digest_service.get(date)
    if not d:
        raise HTTPException(status_code=404, detail="找不到这份日报")
    return d
