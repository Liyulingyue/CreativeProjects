from fastapi import APIRouter, Depends, HTTPException
from sqlalchemy.ext.asyncio import AsyncSession
from sqlalchemy import select

from app.database import get_db
from app.models.backend_config import BackendConfig
from app.models.user import User
from app.schemas.backend_config import BackendConfigCreate, BackendConfigUpdate, BackendConfigResponse
from app.core.security import get_current_user

router = APIRouter(prefix="/api/configs", tags=["configs"])


@router.get("", response_model=list[BackendConfigResponse])
async def get_configs(
    current_user: User = Depends(get_current_user),
    db: AsyncSession = Depends(get_db)
):
    result = await db.execute(select(BackendConfig).where(BackendConfig.user_id == current_user.id))
    return result.scalars().all()


@router.post("", response_model=BackendConfigResponse)
async def create_config(
    config: BackendConfigCreate,
    current_user: User = Depends(get_current_user),
    db: AsyncSession = Depends(get_db)
):
    db_config = BackendConfig(user_id=current_user.id, **config.model_dump())
    db.add(db_config)
    await db.commit()
    await db.refresh(db_config)
    return db_config


@router.put("/{config_id}", response_model=BackendConfigResponse)
async def update_config(
    config_id: str,
    config: BackendConfigUpdate,
    current_user: User = Depends(get_current_user),
    db: AsyncSession = Depends(get_db)
):
    result = await db.execute(
        select(BackendConfig).where(BackendConfig.id == config_id, BackendConfig.user_id == current_user.id)
    )
    db_config = result.scalar_one_or_none()
    if not db_config:
        raise HTTPException(status_code=404, detail="Config not found")

    for key, value in config.model_dump(exclude_unset=True).items():
        setattr(db_config, key, value)

    await db.commit()
    await db.refresh(db_config)
    return db_config


@router.delete("/{config_id}")
async def delete_config(
    config_id: str,
    current_user: User = Depends(get_current_user),
    db: AsyncSession = Depends(get_db)
):
    result = await db.execute(
        select(BackendConfig).where(BackendConfig.id == config_id, BackendConfig.user_id == current_user.id)
    )
    db_config = result.scalar_one_or_none()
    if not db_config:
        raise HTTPException(status_code=404, detail="Config not found")

    await db.delete(db_config)
    await db.commit()
    return {"ok": True}
