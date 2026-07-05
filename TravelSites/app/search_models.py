from datetime import datetime, date, timedelta
from typing import Optional
from pydantic import BaseModel, Field

from .models import MatrixCellResponse


class SearchRequest(BaseModel):
    start_date: Optional[str] = Field(default=None, description="出发日期 YYYY-MM-DD，可选")
    end_date: Optional[str] = Field(default=None, description="返回日期 YYYY-MM-DD，可选")
    duration: Optional[int] = Field(default=None, description="游玩天数（无日期时必填）")
    style: Optional[str] = Field(default="standard", description="攻略风格：standard/family/budget")
    sort_by: Optional[str] = Field(default="score", description="排序：score/weather/preference")
    preference: Optional[str] = Field(default="", description="用户偏好描述")
    origin_province: Optional[str] = Field(default="北京市", description="出发地省")
    origin_city: Optional[str] = Field(default="北京市", description="出发地市")
    origin_county: Optional[str] = Field(default="朝阳区", description="出发地县/区")


class SearchResultItem(BaseModel):
    source: str = Field(default="matrix", description="matrix=实时方案, guide=静态攻略")
    city: str
    start_date: str = ""
    end_date: str = ""
    duration_days: int = 0
    score: int = 0
    recommendation: str = ""
    weather_summary: str = ""
    weather_desc: str = ""
    top_attractions: list[str] = Field(default_factory=list)
    key_highlights: str = ""
    score_breakdown: dict = Field(default_factory=dict)
    daily_plan: list[dict] = Field(default_factory=list)
    preference_score: Optional[float] = Field(default=0)
    distance_km: Optional[float] = Field(default=0)
    transport_mode: Optional[str] = Field(default="")
    transit_hours: Optional[float] = Field(default=0)
    blurb: Optional[str] = Field(default=None, description="城市简介（攻略用）")
    tags: list[str] = Field(default_factory=list, description="城市标签（攻略用）")


class SearchResponse(BaseModel):
    source: str = Field(default="matrix", description="matrix=实时方案, guide=静态攻略")
    items: list[SearchResultItem]
    total: int
    generated_at: str


class PlanRecommendRequest(BaseModel):
    city: str = Field(..., description="城市名")
    start_date: Optional[str] = Field(default=None, description="出发日期 YYYY-MM-DD")
    end_date: Optional[str] = Field(default=None, description="返回日期 YYYY-MM-DD")
    preference_tags: Optional[list[str]] = Field(default_factory=list, description="偏好标签列表: 亲子/美食/户外/人文/自然/放松")
    preference_text: Optional[str] = Field(default=None, description="用户自由文本描述需求")


class PlanRecommendResponse(BaseModel):
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
    daily_plan: list[dict]
    success: bool
    error: Optional[str] = None
    generated_at: str
