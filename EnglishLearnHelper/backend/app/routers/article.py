from fastapi import APIRouter, Body

router = APIRouter(prefix="/article", tags=["article"])

@router.post("")
def generate_article(words: list[str] = Body(...)):
    from app.services.ai_service import generate_article
    return generate_article(words)
