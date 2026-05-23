import logging
from datetime import datetime

from fastapi import APIRouter, Depends
from pydantic import BaseModel
from sqlalchemy.ext.asyncio import AsyncSession
from sqlalchemy import select

from app.database import get_db
from app.models.backend_config import BackendConfig
from app.models.session import Session
from app.models.user import User
from app.schemas.backend_config import BackendConfigSync
from app.schemas.session import SessionSync
from app.core.security import get_current_user

logger = logging.getLogger(__name__)

router = APIRouter(prefix="/api/sync", tags=["sync"])


class SyncRequest(BaseModel):
    backends: list[BackendConfigSync]
    sessions: list[SessionSync]


class SyncResponse(BaseModel):
    backends: list[BackendConfigSync]
    sessions: list[SessionSync]


@router.get("", response_model=SyncResponse)
async def get_sync_data(
    current_user: User = Depends(get_current_user),
    db: AsyncSession = Depends(get_db)
):
    result_backends = await db.execute(
        select(BackendConfig).where(BackendConfig.user_id == current_user.id)
    )
    result_sessions = await db.execute(
        select(Session).where(Session.user_id == current_user.id)
    )

    backends_out = [
        BackendConfigSync(
            id=b.id,
            backend_url=b.backend_url,
            username=b.username,
            auth_token=b.auth_token,
            remark=b.remark,
        )
        for b in result_backends.scalars().all()
    ]

    sessions_out = [
        SessionSync(
            id=s.id,
            title=s.title,
            backend_url=s.backend_url,
            backend_id=s.backend_id,
            directory=s.directory,
            remote_session_id=s.remote_session_id,
            preferred_model=s.preferred_model,
            last_access=s.last_access,
        )
        for s in result_sessions.scalars().all()
    ]

    logger.info(f"[SYNC GET] user={current_user.username}, backends={len(backends_out)}, sessions={len(sessions_out)}")
    return SyncResponse(backends=backends_out, sessions=sessions_out)


@router.put("", response_model=SyncResponse)
async def push_sync_data(
    data: SyncRequest,
    current_user: User = Depends(get_current_user),
    db: AsyncSession = Depends(get_db)
):
    logger.info(f"[SYNC PUT] user={current_user.username}({current_user.id})")
    logger.info(f"[SYNC PUT] backends count={len(data.backends)}, items={data.backends}")
    logger.info(f"[SYNC PUT] sessions count={len(data.sessions)}, items={data.sessions}")

    backend_ids = [b.id for b in data.backends]
    session_ids = [s.id for s in data.sessions]

    existing_backends = await db.execute(
        select(BackendConfig).where(BackendConfig.user_id == current_user.id)
    )
    existing_backend_map = {b.id: b for b in existing_backends.scalars().all()}

    existing_sessions = await db.execute(
        select(Session).where(Session.user_id == current_user.id)
    )
    existing_session_map = {s.id: s for s in existing_sessions.scalars().all()}

    for b in data.backends:
        if b.id in existing_backend_map:
            db_config = existing_backend_map[b.id]
            db_config.backend_url = b.backend_url
            db_config.username = b.username
            db_config.auth_token = b.auth_token
            db_config.remark = b.remark
            db_config.updated_at = datetime.utcnow()
        else:
            db_config = BackendConfig(
                id=b.id,
                user_id=current_user.id,
                backend_url=b.backend_url,
                username=b.username,
                auth_token=b.auth_token,
                remark=b.remark,
            )
            db.add(db_config)

    for b_id in existing_backend_map:
        if b_id not in backend_ids:
            await db.delete(existing_backend_map[b_id])

    for s in data.sessions:
        if s.id in existing_session_map:
            db_session = existing_session_map[s.id]
            db_session.title = s.title
            db_session.backend_url = s.backend_url
            db_session.backend_id = s.backend_id
            db_session.directory = s.directory
            db_session.remote_session_id = s.remote_session_id
            db_session.preferred_model = s.preferred_model
            db_session.last_access = s.last_access
            db_session.updated_at = datetime.utcnow()
        else:
            db_session = Session(
                id=s.id,
                user_id=current_user.id,
                title=s.title,
                backend_url=s.backend_url,
                backend_id=s.backend_id,
                directory=s.directory,
                remote_session_id=s.remote_session_id,
                preferred_model=s.preferred_model,
                last_access=s.last_access,
            )
            db.add(db_session)

    for s_id in existing_session_map:
        if s_id not in session_ids:
            await db.delete(existing_session_map[s_id])

    await db.commit()

    result_backends = await db.execute(
        select(BackendConfig).where(BackendConfig.user_id == current_user.id)
    )
    result_sessions = await db.execute(
        select(Session).where(Session.user_id == current_user.id)
    )

    backends_out = [
        BackendConfigSync(
            id=b.id,
            backend_url=b.backend_url,
            username=b.username,
            auth_token=b.auth_token,
            remark=b.remark,
        )
        for b in result_backends.scalars().all()
    ]

    sessions_out = [
        SessionSync(
            id=s.id,
            title=s.title,
            backend_url=s.backend_url,
            backend_id=s.backend_id,
            directory=s.directory,
            remote_session_id=s.remote_session_id,
            preferred_model=s.preferred_model,
            last_access=s.last_access,
        )
        for s in result_sessions.scalars().all()
    ]

    logger.info(f"[SYNC PUT] returning backends={len(backends_out)}, sessions={len(sessions_out)}")
    return SyncResponse(backends=backends_out, sessions=sessions_out)
