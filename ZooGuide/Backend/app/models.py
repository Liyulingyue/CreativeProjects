from __future__ import annotations

from typing import Literal, Optional

from pydantic import BaseModel, Field


PartyType = Literal["solo", "couple", "family_young", "family_teen", "seniors"]
Gate = Literal["north", "south", "east"]
InterestTag = Literal[
    "panda",
    "ape",
    "cat",
    "bird",
    "australian",
    "african",
    "local",
    "exotic",
    "kids_favorite",
]


class UserPreference(BaseModel):
    available_hours: float = Field(..., ge=0.5, le=12)
    party_type: PartyType
    with_kids: bool = False
    kids_age: Optional[int] = Field(None, ge=0, le=18)
    stamina: int = Field(..., ge=1, le=5)
    sun_tolerance: int = Field(..., ge=1, le=5)
    willing_to_hike: bool = True
    animal_interests: list[InterestTag] = Field(default_factory=list)
    entry_gate: Gate = "north"
    start_time: str = "09:00"
    dynamic_feedback: Optional[str] = None
    current_venue_id: Optional[str] = None
    elapsed_minutes: int = 0


class PlanRequest(UserPreference):
    pass


class ReplanRequest(BaseModel):
    original_route: dict
    current_venue_id: Optional[str] = None
    elapsed_minutes: int = 0
    feedback: str


class VenueBrief(BaseModel):
    id: str
    name: str
    animals: list[str]
    tags: list[str]
    themes: list[str]
    recommended_visit_minutes: int
    kid_friendly: int
    photo_op: int
    must_see: bool
    shaded: bool
    rest_spots: bool


class RouteStop(BaseModel):
    venue_id: str
    venue_name: str
    arrive_time: str
    leave_time: str
    visit_minutes: int
    walk_to_next_minutes: int
    narration: str
    tips: list[str] = Field(default_factory=list)
    rest_here: bool = False


class Route(BaseModel):
    id: str
    summary: str
    total_minutes: int
    total_walk_minutes: int
    stops: list[RouteStop]
    warnings: list[str] = Field(default_factory=list)
    tips: list[str] = Field(default_factory=list)
    fallback: bool = False


class QuizOptions(BaseModel):
    party_types: list[dict]
    interests: list[dict]
    gates: list[dict]
    stamina_descriptions: dict[int, str]
    sun_descriptions: dict[int, str]


class Venue(BaseModel):
    id: str
    name: str
    area: str
    near_gate: Optional[str] = None
    open_time: str
    close_time: str
    animals: list[str]
    tags: list[str]
    themes: list[str]
    description: str
    recommended_visit_minutes: int
    rest_spots: bool
    shaded: bool
    kid_friendly: int
    photo_op: int
    must_see: bool