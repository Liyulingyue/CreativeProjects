import logging
import random

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
    container_name,
    is_running,
    start_container as docker_start,
    stop_container as docker_stop,
)
from app.config import settings

logger = logging.getLogger(__name__)

router = APIRouter(prefix="/api/containers", tags=["containers"])


async def _allocate_port(db: AsyncSession) -> int:
    result = await db.execute(select(Container.port).where(Container.port.isnot(None)))
    used = set(int(r[0]) for r in result.all() if r[0] is not None)
    for _ in range(1000):
        port = random.randint(32000, 60000)
        if port not in used and (port + 1) not in used:
            return port
    raise RuntimeError("无法分配空闲端口，请稍后重试")


@router.get("", response_model=list[ContainerResponse])
async def list_containers(
    current_user: User = Depends(get_current_user),
    db: AsyncSession = Depends(get_db),
):
    result = await db.execute(
        select(Container).where(Container.user_id == current_user.id)
    )
    containers = result.scalars().all()

    for container in containers:
        docker_status = "running" if is_running(container.id) else "stopped"
        if container.status != docker_status:
            container.status = docker_status

    await db.commit()
    return containers


@router.post("", response_model=ContainerResponse)
async def create_container(
    body: ContainerCreate | None = None,
    current_user: User = Depends(get_current_user),
    db: AsyncSession = Depends(get_db),
):
    if not settings.CREATE_CONTAINERS:
        raise HTTPException(status_code=403, detail="当前服务器不支持创建远程环境")

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
    port = await _allocate_port(db)
    creds = generate_credentials()
    server_host = settings.SERVER_HOST

    container = Container(
        user_id=current_user.id,
        name=name,
        port=port,
        opencode_url=f"http://{server_host}:{port}",
        opencode_username=creds["opencode_username"],
        opencode_password=creds["opencode_password"],
        filebrowser_url=f"http://{server_host}:{port + 1}",
        fb_username=creds["fb_username"],
        fb_password=creds["fb_password"],
        status="stopped",
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

    if not settings.CREATE_CONTAINERS:
        raise HTTPException(status_code=403, detail="当前服务器不支持创建远程环境")

    if is_running(container.id):
        container.status = "running"
        await db.commit()
        await db.refresh(container)
        return container

    container.status = "starting"
    await db.commit()

    try:
        credentials = generate_credentials()
        logger.info(f"[Container] Generated credentials: opencode={credentials['opencode_username']}/{credentials['opencode_password']}, fb={credentials['fb_username']}/{credentials['fb_password']}")

        docker_id = docker_start(
            container_id=container.id,
            port=int(container.port),
            credentials=credentials,
        )

        container.container_id = docker_id
        container.opencode_username = credentials["opencode_username"]
        container.opencode_password = credentials["opencode_password"]
        container.opencode_url = f"http://{settings.SERVER_HOST}:{container.port}"
        container.fb_username = credentials["fb_username"]
        container.fb_password = credentials["fb_password"]
        container.filebrowser_url = f"http://{settings.SERVER_HOST}:{int(container.port) + 1}"
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
        docker_stop(container.id)
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
            docker_stop(container.id)
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
