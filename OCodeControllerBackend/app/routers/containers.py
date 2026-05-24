import logging
import random
import socket

from fastapi import APIRouter, Depends, HTTPException
from sqlalchemy.ext.asyncio import AsyncSession
from sqlalchemy import select, func

from app.database import get_db
from app.models.container import Container
from app.models.user import PLAN_LIMITS, User
from app.schemas.container import ContainerCreate, ContainerResponse
from app.core.security import get_current_user
from app.core.docker import (
    generate_credentials,
    start_container,
    stop_container,
    remove_container,
    is_running,
    container_name,
)
from app.config import settings

logger = logging.getLogger(__name__)

router = APIRouter(prefix="/api/containers", tags=["containers"])


async def _find_free_port(db: AsyncSession, start: int = 32000, end: int = 60000) -> int:
    used_ports: set[int] = set()
    for offset in (0, 1):
        result = await db.execute(select(Container.port + offset).where(Container.port.isnot(None)))
        for (p,) in result.all():
            used_ports.add(p)

    attempts = 0
    while True:
        port = random.randint(start, end)
        if port in used_ports:
            attempts += 1
            if attempts > 1000:
                raise RuntimeError("无法找到空闲端口，请稍后重试")
            continue
        with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
            try:
                s.bind(("0.0.0.0", port))
                return port
            except OSError:
                attempts += 1
                if attempts > 1000:
                    raise RuntimeError("无法找到空闲端口，请稍后重试")


@router.get("", response_model=list[ContainerResponse])
async def list_containers(
    current_user: User = Depends(get_current_user),
    db: AsyncSession = Depends(get_db),
):
    result = await db.execute(
        select(Container).where(Container.user_id == current_user.id)
    )
    return result.scalars().all()


@router.post("", response_model=ContainerResponse)
async def create_container(
    body: ContainerCreate | None = None,
    current_user: User = Depends(get_current_user),
    db: AsyncSession = Depends(get_db),
):
    max_allowed = PLAN_LIMITS.get(current_user.plan, 0)
    result = await db.execute(
        select(func.count(Container.id)).where(Container.user_id == current_user.id)
    )
    count = result.scalar() or 0
    if count >= max_allowed:
        raise HTTPException(
            status_code=403,
            detail=f"当前套餐({current_user.plan})最多{max_allowed}个容器",
        )

    name = body.name if body else "未命名环境"
    port = await _find_free_port(db)

    # TODO: 考虑在创建时直接启动容器（合并 create + start），减少一次请求
    container = Container(
        user_id=current_user.id,
        name=name,
        port=port,
        status="created",
    )
    db.add(container)
    await db.commit()
    await db.refresh(container)
    logger.info(f"[Container] Created record id={container.id} for user={current_user.username}")
    return container


@router.post("/{container_id}/start", response_model=ContainerResponse)
async def start_container_endpoint(
    container_id: str,
    current_user: User = Depends(get_current_user),
    db: AsyncSession = Depends(get_db),
):
    result = await db.execute(
        select(Container).where(Container.id == container_id, Container.user_id == current_user.id)
    )
    container = result.scalar_one_or_none()
    if not container:
        raise HTTPException(status_code=404, detail="Container not found")

    if is_running(container.id):
        container.status = "running"
        await db.commit()
        await db.refresh(container)
        return container

    container.status = "starting"
    await db.commit()

    try:
        credentials = generate_credentials()

        docker_id = start_container(
            container_id=container.id,
            port=container.port,
            credentials=credentials,
        )

        container.container_id = docker_id
        container.opencode_username = credentials["opencode_username"]
        container.opencode_password = credentials["opencode_password"]
        container.opencode_url = f"http://{settings.SERVER_HOST}:{container.port}"
        container.fb_username = credentials["fb_username"]
        container.fb_password = credentials["fb_password"]
        container.filebrowser_url = f"http://{settings.SERVER_HOST}:{container.port + 1}"
        container.status = "running"
        await db.commit()
        await db.refresh(container)
        logger.info(f"[Container] Started id={container_id}, url={container.opencode_url}")
        return container
    except Exception as e:
        container.status = "failed"
        await db.commit()
        await db.refresh(container)
        logger.error(f"[Container] Failed to start id={container_id}: {e}")
        raise HTTPException(status_code=500, detail=f"启动容器失败: {e}")


@router.post("/{container_id}/stop", response_model=ContainerResponse)
async def stop_container_endpoint(
    container_id: str,
    current_user: User = Depends(get_current_user),
    db: AsyncSession = Depends(get_db),
):
    result = await db.execute(
        select(Container).where(Container.id == container_id, Container.user_id == current_user.id)
    )
    container = result.scalar_one_or_none()
    if not container:
        raise HTTPException(status_code=404, detail="Container not found")

    if not is_running(container.id):
        container.status = "stopped"
        await db.commit()
        await db.refresh(container)
        return container

    container.status = "stopping"
    await db.commit()

    try:
        stop_container(container.id)
        container.status = "stopped"
        container.container_id = None
        await db.commit()
        await db.refresh(container)
        logger.info(f"[Container] Stopped id={container_id}")
        return container
    except Exception as e:
        logger.error(f"[Container] Failed to stop id={container_id}: {e}")
        raise HTTPException(status_code=500, detail=f"停止容器失败: {e}")


@router.delete("/{container_id}")
async def delete_container_endpoint(
    container_id: str,
    current_user: User = Depends(get_current_user),
    db: AsyncSession = Depends(get_db),
):
    result = await db.execute(
        select(Container).where(Container.id == container_id, Container.user_id == current_user.id)
    )
    container = result.scalar_one_or_none()
    if not container:
        raise HTTPException(status_code=404, detail="Container not found")

    logger.info(f"[Container] Deleting id={container_id}, status={container.status}")

    try:
        if is_running(container.id):
            stop_container(container.id)
    except Exception as e:
        logger.warning(f"[Container] Failed to stop before delete id={container_id}: {e}")

    try:
        remove_container(container.id)
    except Exception:
        pass

    await db.delete(container)
    await db.commit()
    logger.info(f"[Container] Deleted id={container_id}")
    return {"ok": True}
