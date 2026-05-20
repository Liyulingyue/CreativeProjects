from datetime import datetime

from fastapi import APIRouter, Depends, HTTPException
from sqlalchemy.ext.asyncio import AsyncSession
from sqlalchemy import select

from app.database import get_db
from app.models.device import Device
from app.models.user import User
from app.schemas.device import DeviceCreate, DeviceResponse
from app.core.security import get_current_user

router = APIRouter(prefix="/api/devices", tags=["devices"])


@router.get("", response_model=list[DeviceResponse])
async def get_devices(
    current_user: User = Depends(get_current_user),
    db: AsyncSession = Depends(get_db)
):
    result = await db.execute(select(Device).where(Device.user_id == current_user.id))
    return result.scalars().all()


@router.post("", response_model=DeviceResponse)
async def register_device(
    device: DeviceCreate,
    current_user: User = Depends(get_current_user),
    db: AsyncSession = Depends(get_db)
):
    result = await db.execute(select(Device).where(Device.device_id == device.device_id))
    db_device = result.scalar_one_or_none()

    if db_device:
        db_device.last_login = datetime.utcnow()
        if device.device_name:
            db_device.device_name = device.device_name
    else:
        db_device = Device(
            user_id=current_user.id,
            device_id=device.device_id,
            device_name=device.device_name,
            last_login=datetime.utcnow()
        )
        db.add(db_device)

    await db.commit()
    await db.refresh(db_device)
    return db_device


@router.delete("/{device_id}")
async def delete_device(
    device_id: str,
    current_user: User = Depends(get_current_user),
    db: AsyncSession = Depends(get_db)
):
    result = await db.execute(select(Device).where(Device.id == device_id, Device.user_id == current_user.id))
    db_device = result.scalar_one_or_none()
    if not db_device:
        raise HTTPException(status_code=404, detail="Device not found")

    await db.delete(db_device)
    await db.commit()
    return {"ok": True}
