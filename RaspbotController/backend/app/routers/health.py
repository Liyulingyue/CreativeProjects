from fastapi import APIRouter
from ..schemas import HealthResponse

router = APIRouter(prefix="/api/v1", tags=["Health"])


@router.get("/health", response_model=HealthResponse)
async def health():
    return HealthResponse(car=True, camera=True)
