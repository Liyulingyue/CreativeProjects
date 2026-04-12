from fastapi import APIRouter, Body
from app.services.config_service import get_config, update_config

router = APIRouter(prefix="/config", tags=["config"])


@router.get("")
def get_model_config():
    return get_config()


@router.post("")
def set_model_config(
    url: str = Body(None),
    key: str = Body(None),
    model: str = Body(None),
    baidu_key: str = Body(None)
):
    return update_config(url=url, key=key, model=model, baidu_key=baidu_key)
