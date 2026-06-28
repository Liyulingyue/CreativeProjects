from datetime import datetime, date, timedelta
from typing import Optional
from pydantic import BaseModel, Field

from .models import MatrixCellResponse


class SearchRequest(BaseModel):
    start_date: str = Field(..., description="出发日期 YYYY-MM-DD")
    end_date: str = Field(..., description="返回日期 YYYY-MM-DD")


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


class SearchResponse(BaseModel):
    items: list[SearchResultItem]
    total: int
    generated_at: str
