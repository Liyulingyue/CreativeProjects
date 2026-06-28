from datetime import datetime, date, timedelta
from typing import Optional
from pydantic import BaseModel, Field

from .models import MatrixCellResponse


class SearchRequest(BaseModel):
    start_date: str = Field(..., description="出发日期 YYYY-MM-DD")
    end_date: str = Field(..., description="返回日期 YYYY-MM-DD")
    preference: Optional[str] = Field(default="", description="用户偏好描述，如'爬山、看海、带孩子'")


class SearchResultItem(BaseModel):
    city: str
    start_date: str
    end_date: str
    duration_days: int
    score: int
    recommendation: str
    weather_summary: str
    weather_desc: str
    top_attractions: list[str]
    key_highlights: str
    score_breakdown: dict
    daily_plan: list[dict] = Field(default_factory=list)
    preference_score: Optional[float] = Field(default=0, description="偏好匹配分 0-1")


class SearchResponse(BaseModel):
    items: list[SearchResultItem]
    total: int
    generated_at: str
