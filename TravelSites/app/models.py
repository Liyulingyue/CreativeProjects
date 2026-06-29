from datetime import date, datetime
from typing import Optional
from pydantic import BaseModel, Field


class MatrixCellResponse(BaseModel):
    start_offset: int
    duration: int
    start_date: str
    end_date: str
    score: Optional[int] = None
    recommendation: Optional[str] = None
    weather_summary: Optional[str] = None
    success: bool = False
    error: Optional[str] = None
    full_result: Optional[dict] = None


class CityMatrixResponse(BaseModel):
    city: str
    generated_at: str
    cells: list[MatrixCellResponse]
    total: int
    success_count: int


class CityListResponse(BaseModel):
    cities: list[str]
    count: int


class HealthResponse(BaseModel):
    status: str
    refresh_enabled: bool
    seed_cities: list[str]
    cached_cities: int = 0
    cells_total: int = 0
    cache_hit_rate: float = 0
    date_range: Optional[tuple[str, str]] = None  # (min_date, max_date)


class RefreshStatusResponse(BaseModel):
    is_running: bool
    last_run: Optional[str] = None
    cities_completed: int
    cities_total: int
