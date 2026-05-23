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

logger = logging.getLogger(__name__)

router = APIRouter(prefix="/api/containers", tags=["containers"])


@router.get("", response_model=list[ContainerResponse])
async def list_containers(
    current_user: User = Depends(get_current_user),
    db: AsyncSession = Depends(get_db)
):
    result = await db.execute(
        select(Container).where(Container.user_id == current_user.id)
    )
    return result.scalars().all()


@router.post("", response_model=ContainerResponse)
async def create_container(
    body: ContainerCreate | None = None,
    current_user: User = Depends(get_current_user),
    db: AsyncSession = Depends(get_db)
):
    """
    创建容器记录，同时分配宿主机端口。
    TODO(Docker): 生成 opencode_token
    """
    max_allowed = PLAN_LIMITS.get(current_user.plan, 0)
    result = await db.execute(select(func.count(Container.id)).where(Container.user_id == current_user.id))
    count = result.scalar() or 0
    if count >= max_allowed:
        raise HTTPException(status_code=403, detail=f"当前套餐({current_user.plan})最多{max_allowed}个容器")
    port = 32800 + count + random.randint(1, 100)
    name = body.name if body else "未命名环境"
    container = Container(
        user_id=current_user.id,
        name=name,
        port=port,
        opencode_url=f"http://your-server:{port}",
        opencode_token="mock-token",
        status="stopped",
    )
    db.add(container)
    await db.commit()
    await db.refresh(container)
    logger.info(f"[Container] Created record id={container.id} for user={current_user.username}")
    return container


@router.post("/{container_id}/start", response_model=ContainerResponse)
async def start_container(
    container_id: str,
    current_user: User = Depends(get_current_user),
    db: AsyncSession = Depends(get_db)
):
    """
    启动容器。
    TODO(Docker):
      1. 检查是否已运行，若已运行直接返回
      2. 生成 opencode_token
      3. 执行 docker run:
         docker run -d \
           --name opencode-{container_id} \
           -p {container.port}:4096 \
           ghcr.io/anomalyco/opencode/opencode:latest \
           opencode web --hostname 0.0.0.0 --port 4096 --auth {token}
      4. 更新记录: container_id, opencode_url, opencode_token, status="running"
    """
    result = await db.execute(
        select(Container).where(Container.id == container_id, Container.user_id == current_user.id)
    )
    container = result.scalar_one_or_none()
    if not container:
        raise HTTPException(status_code=404, detail="Container not found")

    if container.status == "running":
        return container

    container.status = "starting"
    await db.commit()
    await db.refresh(container)
    logger.info(f"[Container] Starting id={container_id}")

    # TODO: await self._docker_start(container)
    container.status = "running"
    await db.commit()
    await db.refresh(container)
    logger.info(f"[Container] Started id={container_id}, url={container.opencode_url}")
    return container


@router.post("/{container_id}/stop", response_model=ContainerResponse)
async def stop_container(
    container_id: str,
    current_user: User = Depends(get_current_user),
    db: AsyncSession = Depends(get_db)
):
    """
    停止容器。
    TODO(Docker):
      docker stop opencode-{container_id}
      docker rm opencode-{container_id}
      更新 status="stopped"，清空 container_id
    """
    result = await db.execute(
        select(Container).where(Container.id == container_id, Container.user_id == current_user.id)
    )
    container = result.scalar_one_or_none()
    if not container:
        raise HTTPException(status_code=404, detail="Container not found")

    if container.status != "running" and container.status != "starting":
        return container

    container.status = "stopping"
    await db.commit()
    await db.refresh(container)
    logger.info(f"[Container] Stopping id={container_id}")

    # TODO: await self._docker_stop(container)
    container.status = "stopped"
    await db.commit()
    await db.refresh(container)
    logger.info(f"[Container] Stopped id={container_id}")
    return container


@router.delete("/{container_id}")
async def delete_container(
    container_id: str,
    current_user: User = Depends(get_current_user),
    db: AsyncSession = Depends(get_db)
):
    """
    删除容器（先停止再删除）。
    TODO(Docker):
      1. 若正在运行，先 docker stop
      2. docker rm opencode-{container_id}
      3. 归还端口（可选）
      4. 删除数据库记录
    """
    result = await db.execute(
        select(Container).where(Container.id == container_id, Container.user_id == current_user.id)
    )
    container = result.scalar_one_or_none()
    if not container:
        raise HTTPException(status_code=404, detail="Container not found")

    logger.info(f"[Container] Deleting id={container_id}, status={container.status}")

    # TODO: if container.status == "running": await self._docker_stop(container)
    # TODO: docker rm opencode-{container_id}

    await db.delete(container)
    await db.commit()
    logger.info(f"[Container] Deleted id={container_id}")
    return {"ok": True}
