import uuid
from datetime import datetime

from sqlalchemy import String, DateTime, ForeignKey
from sqlalchemy.orm import Mapped, mapped_column, relationship

from app.database import Base


class Session(Base):
    __tablename__ = "sessions"

    id: Mapped[str] = mapped_column(String(36), primary_key=True)
    user_id: Mapped[str] = mapped_column(String(36), ForeignKey("users.id", ondelete="CASCADE"), nullable=False)
    title: Mapped[str] = mapped_column(String(256), nullable=True)
    backend_url: Mapped[str] = mapped_column(String(512), nullable=True)
    backend_id: Mapped[str] = mapped_column(String(36), nullable=True)
    directory: Mapped[str] = mapped_column(String(512), nullable=True)
    remote_session_id: Mapped[str] = mapped_column(String(256), nullable=True)
    preferred_model: Mapped[str] = mapped_column(String(128), nullable=True)
    last_access: Mapped[datetime] = mapped_column(DateTime, nullable=True)
    created_at: Mapped[datetime] = mapped_column(DateTime, default=datetime.utcnow)
    updated_at: Mapped[datetime] = mapped_column(DateTime, default=datetime.utcnow, onupdate=datetime.utcnow)

    user: Mapped["User"] = relationship("User", back_populates="sessions")
