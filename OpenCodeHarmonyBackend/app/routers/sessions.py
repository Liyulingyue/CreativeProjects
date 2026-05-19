from datetime import datetime

from fastapi import APIRouter, Depends
from sqlalchemy.ext.asyncio import AsyncSession
from sqlalchemy import select

from app.database import get_db
from app.models.session import Session
from app.models.user import User
from app.schemas.session import SessionCreate, SessionUpdate, SessionResponse
from app.core.security import get_current_user

router = APIRouter(prefix="/api/sessions", tags=["sessions"])


@router.get("", response_model=list[SessionResponse])
async def get_sessions(
    current_user: User = Depends(get_current_user),
    db: AsyncSession = Depends(get_db)
):
    result = await db.execute(select(Session).where(Session.user_id == current_user.id))
    return result.scalars().all()


@router.post("", response_model=list[SessionResponse])
async def sync_sessions(
    sessions: list[SessionCreate],
    current_user: User = Depends(get_current_user),
    db: AsyncSession = Depends(get_db)
):
    for sess in sessions:
        result = await db.execute(
            select(Session).where(Session.id == sess.id, Session.user_id == current_user.id)
        )
        db_session = result.scalar_one_or_none()

        if db_session:
            if sess.title is not None:
                db_session.title = sess.title
            db_session.updated_at = datetime.utcnow()
        else:
            db_session = Session(user_id=current_user.id, id=sess.id, title=sess.title)
            db.add(db_session)

    await db.commit()

    result = await db.execute(select(Session).where(Session.user_id == current_user.id))
    return result.scalars().all()


@router.put("/{session_id}", response_model=SessionResponse)
async def update_session(
    session_id: str,
    session: SessionUpdate,
    current_user: User = Depends(get_current_user),
    db: AsyncSession = Depends(get_db)
):
    result = await db.execute(
        select(Session).where(Session.id == session_id, Session.user_id == current_user.id)
    )
    db_session = result.scalar_one_or_none()
    if not db_session:
        raise HTTPException(status_code=404, detail="Session not found")

    if session.title is not None:
        db_session.title = session.title
    db_session.updated_at = datetime.utcnow()

    await db.commit()
    await db.refresh(db_session)
    return db_session
