from fastapi import APIRouter, Query
from typing import Optional
from app.services import vocab_service
from app.models.vocabulary import Vocabulary

router = APIRouter(prefix="/vocabulary", tags=["vocabulary"])

@router.get("")
def get_vocabulary(
    page: int = Query(1, ge=1),
    page_size: int = Query(50, ge=1, le=100)
):
    return vocab_service.get_vocab_by_page(page, page_size)

@router.get("/search")
def search_vocabulary(q: str = Query(..., min_length=1)):
    return vocab_service.search_vocab(q)

@router.get("/count")
def get_count():
    vocab_list = vocab_service.get_all_vocab()
    return {"total": len(vocab_list)}

@router.get("/random")
def get_random_vocabulary(count: int = Query(10, ge=1, le=100)):
    return {"items": vocab_service.get_random_vocab(count)}
