from fastapi import APIRouter, HTTPException
from pydantic import BaseModel
from typing import Optional

from backend.process_manager import manager
from backend.proxy import proxy

router = APIRouter(prefix="/api/inference", tags=["inference"])


class InferenceRequest(BaseModel):
    master_node_id: str
    prompt: str
    max_tokens: int = 20


@router.post("")
async def inference(req: InferenceRequest):
    master_url = manager.get_master_url(req.master_node_id)
    if not master_url:
        raise HTTPException(status_code=404, detail=f"Master '{req.master_node_id}' not found")

    body = {
        'prompt': req.prompt,
        'max_tokens': req.max_tokens,
    }

    status_code, data = proxy.post(master_url, "/inference", json_data=body)

    if status_code != 200:
        raise HTTPException(status_code=status_code, detail=data if isinstance(data, str) else data.get('error', 'Unknown error'))

    return data
