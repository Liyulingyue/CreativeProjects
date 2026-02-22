#!/usr/bin/env python
"""db.py — Chat history database layer using SQLite."""
import sqlite3
import json
from pathlib import Path
from typing import List, Dict, Optional
from datetime import datetime
from ..core.settings import settings


class ChatDatabase:
    """SQLite-based chat message storage."""
    
    def __init__(self, db_path: Optional[str] = None):
        """Initialize database with optional custom path."""
        if db_path is None:
            db_path = str(settings.BASE_DIR / "data" / "chat_history.db")
        
        self.db_path = Path(db_path)
        self.db_path.parent.mkdir(parents=True, exist_ok=True)
        self._init_db()
    
    def _init_db(self):
        """Create tables if not exist."""
        conn = sqlite3.connect(self.db_path)
        conn.row_factory = sqlite3.Row
        cursor = conn.cursor()
        
        # Main chat messages table
        cursor.execute("""
            CREATE TABLE IF NOT EXISTS chat_messages (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                session_id TEXT NOT NULL,
                workspace TEXT,
                role TEXT NOT NULL,
                content TEXT NOT NULL,
                metadata TEXT,
                timestamp DATETIME DEFAULT CURRENT_TIMESTAMP
            )
        """)
        
        # Index for fast lookup by session_id and workspace
        cursor.execute("""
            CREATE INDEX IF NOT EXISTS idx_session_workspace 
            ON chat_messages(session_id, workspace)
        """)
        
        # Index for timestamp-based queries
        cursor.execute("""
            CREATE INDEX IF NOT EXISTS idx_timestamp 
            ON chat_messages(timestamp)
        """)
        
        # Feedback table for user evaluations
        # Stores user feedback (thumbs up/down) for training data collection
        # None required fields: feedback value can be any string ('👍', '👎', 'good', 'bad', etc.)
        cursor.execute("""
            CREATE TABLE IF NOT EXISTS message_feedback (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                message_id INTEGER NOT NULL,
                session_id TEXT NOT NULL,
                feedback TEXT NOT NULL,
                comment TEXT,
                timestamp DATETIME DEFAULT CURRENT_TIMESTAMP,
                FOREIGN KEY (message_id) REFERENCES chat_messages(id)
            )
        """)
        
        # Try to add comment column if it doesn't exist (for migration)
        try:
            cursor.execute("ALTER TABLE message_feedback ADD COLUMN comment TEXT")
        except:
            pass
        
        # Index for fast lookup by message_id and session_id
        cursor.execute("""
            CREATE INDEX IF NOT EXISTS idx_feedback_message 
            ON message_feedback(message_id)
        """)
        
        cursor.execute("""
            CREATE INDEX IF NOT EXISTS idx_feedback_session 
            ON message_feedback(session_id)
        """)
        
        conn.commit()
        conn.close()
    
    def save_message(
        self,
        session_id: str,
        role: str,
        content: str,
        workspace: Optional[str] = None,
        metadata: Optional[Dict] = None
    ) -> int:
        """Save a single chat message. Returns message ID."""
        conn = sqlite3.connect(self.db_path)
        cursor = conn.cursor()
        
        metadata_str = json.dumps(metadata) if metadata else None
        
        cursor.execute("""
            INSERT INTO chat_messages (session_id, role, content, workspace, metadata)
            VALUES (?, ?, ?, ?, ?)
        """, (session_id, role, content, workspace, metadata_str))
        
        msg_id = cursor.lastrowid
        conn.commit()
        conn.close()
        return msg_id
    
    def save_messages(
        self,
        session_id: str,
        messages: List[Dict],
        workspace: Optional[str] = None
    ) -> List[Dict]:
        """Batch save multiple messages. Returns list of saved messages with their IDs.
        
        Automatically moves extra fields to metadata.
        """
        conn = sqlite3.connect(self.db_path)
        cursor = conn.cursor()
        
        saved_messages = []
        for msg in messages:
            # Separate core fields from metadata
            role = msg.get("role", "")
            content = msg.get("content", "")
            
            # Put everything else in metadata
            rest = {k: v for k, v in msg.items() if k not in ("role", "content", "session_id", "workspace", "id")}
            metadata_str = json.dumps(rest) if rest else None
            
            cursor.execute("""
                INSERT INTO chat_messages (session_id, role, content, workspace, metadata)
                VALUES (?, ?, ?, ?, ?)
            """, (
                session_id,
                role,
                content,
                workspace,
                metadata_str
            ))
            
            msg_id = cursor.lastrowid
            # Return message with assigned ID
            saved_msg = {
                "id": msg_id,
                "session_id": session_id,
                "workspace": workspace,
                "role": role,
                "content": content,
                "metadata": metadata_str
            }
            saved_messages.append(saved_msg)
        
        conn.commit()
        conn.close()
        return saved_messages
    
    def get_messages(
        self,
        session_id: str,
        workspace: Optional[str] = None,
        limit: int = 100,
        offset: int = 0
    ) -> List[Dict]:
        """Retrieve chat messages with feedback, flattening metadata back into the object."""
        conn = sqlite3.connect(self.db_path)
        conn.row_factory = sqlite3.Row
        cursor = conn.cursor()
        
        if workspace:
            cursor.execute("""
                SELECT cm.id, cm.session_id, cm.workspace, cm.role, cm.content, cm.metadata, cm.timestamp,
                       mf.feedback, mf.comment AS feedback_comment
                FROM chat_messages cm
                LEFT JOIN message_feedback mf ON cm.id = mf.message_id
                WHERE cm.session_id = ? AND cm.workspace = ?
                ORDER BY cm.timestamp ASC
                LIMIT ? OFFSET ?
            """, (session_id, workspace, limit, offset))
        else:
            cursor.execute("""
                SELECT cm.id, cm.session_id, cm.workspace, cm.role, cm.content, cm.metadata, cm.timestamp,
                       mf.feedback, mf.comment AS feedback_comment
                FROM chat_messages cm
                LEFT JOIN message_feedback mf ON cm.id = mf.message_id
                WHERE cm.session_id = ?
                ORDER BY cm.timestamp ASC
                LIMIT ? OFFSET ?
            """, (session_id, limit, offset))
        
        rows = cursor.fetchall()
        conn.close()
        
        results = []
        for row in rows:
            msg = {
                "id": row["id"],
                "session_id": row["session_id"],
                "workspace": row["workspace"],
                "role": row["role"],
                "content": row["content"],
                "metadata": row["metadata"],
                "timestamp": row["timestamp"],
                "feedback": row["feedback"],
                "feedback_comment": row["feedback_comment"]
            }
            
            results.append(msg)
            
        return results
    
    def delete_messages(
        self,
        session_id: str,
        workspace: Optional[str] = None
    ) -> int:
        """Delete messages for a session, optionally filtered by workspace. Returns count deleted."""
        conn = sqlite3.connect(self.db_path)
        cursor = conn.cursor()
        
        if workspace:
            cursor.execute("""
                DELETE FROM chat_messages
                WHERE session_id = ? AND workspace = ?
            """, (session_id, workspace))
        else:
            cursor.execute("""
                DELETE FROM chat_messages
                WHERE session_id = ?
            """, (session_id,))
        
        count = cursor.rowcount
        conn.commit()
        conn.close()
        return count
    
    def clear_old_messages(self, days: int = 30) -> int:
        """Delete messages older than specified days. Returns count deleted."""
        conn = sqlite3.connect(self.db_path)
        cursor = conn.cursor()
        
        cursor.execute("""
            DELETE FROM chat_messages
            WHERE datetime(timestamp) < datetime('now', ? || ' days')
        """, (f"-{days}",))
        
        count = cursor.rowcount
        conn.commit()
        conn.close()
        return count
    
    def get_sessions(self) -> List[Dict]:
        """Get list of all unique session IDs with latest activity and feedback stats."""
        conn = sqlite3.connect(self.db_path)
        conn.row_factory = sqlite3.Row
        cursor = conn.cursor()
        
        # Use ROW_NUMBER to get only the latest message for each session
        # LEFT JOIN with feedback to get good/bad counts
        cursor.execute("""
            SELECT m.session_id, m.workspace, m.timestamp AS last_activity,
                   COALESCE(SUM(CASE WHEN f.feedback == '👍' THEN 1 ELSE 0 END), 0) AS good_count,
                   COALESCE(SUM(CASE WHEN f.feedback == '👎' THEN 1 ELSE 0 END), 0) AS bad_count
            FROM (
                SELECT session_id, workspace, timestamp,
                       ROW_NUMBER() OVER (PARTITION BY session_id ORDER BY timestamp DESC) AS rn
                FROM chat_messages
            ) m
            LEFT JOIN message_feedback f ON m.session_id = f.session_id
            WHERE m.rn = 1
            GROUP BY m.session_id
            ORDER BY m.timestamp DESC
        """)
        rows = cursor.fetchall()
        conn.close()
        return [
            {
                "session_id": row["session_id"],
                "workspace": row["workspace"],
                "last_activity": row["last_activity"],
                "good_count": int(row["good_count"]),
                "bad_count": int(row["bad_count"]),
            }
            for row in rows
        ]
    
    def get_message_count(
        self,
        session_id: str,
        workspace: Optional[str] = None
    ) -> int:
        """Get count of messages for a session."""
        conn = sqlite3.connect(self.db_path)
        cursor = conn.cursor()
        
        if workspace:
            cursor.execute("""
                SELECT COUNT(*) FROM chat_messages
                WHERE session_id = ? AND workspace = ?
            """, (session_id, workspace))
        else:
            cursor.execute("""
                SELECT COUNT(*) FROM chat_messages
                WHERE session_id = ?
            """, (session_id,))
        
        count = cursor.fetchone()[0]
        conn.close()
        return count
    
    def save_feedback(
        self,
        message_id: int,
        session_id: str,
        feedback: str,
        comment: Optional[str] = None
    ) -> int:
        """Save or update user feedback for a message (e.g. '👍', '👎') with optional comment.
        
        If feedback already exists for this message, it will be updated.
        Otherwise, a new feedback record will be created.
        Used for training data collection and model improvement.
        Returns the feedback record ID.
        """
        conn = sqlite3.connect(self.db_path)
        cursor = conn.cursor()
        
        # Check if feedback already exists for this message
        cursor.execute("""
            SELECT id FROM message_feedback
            WHERE message_id = ?
        """, (message_id,))
        
        existing = cursor.fetchone()
        
        if existing:
            # Update existing feedback
            feedback_id = existing[0]
            cursor.execute("""
                UPDATE message_feedback
                SET feedback = ?, comment = ?, timestamp = CURRENT_TIMESTAMP
                WHERE id = ?
            """, (feedback, comment, feedback_id))
            print(f"[DEBUG] Updated feedback for message {message_id}: {feedback}")
        else:
            # Insert new feedback
            cursor.execute("""
                INSERT INTO message_feedback (message_id, session_id, feedback, comment)
                VALUES (?, ?, ?, ?)
            """, (message_id, session_id, feedback, comment))
            feedback_id = cursor.lastrowid
            print(f"[DEBUG] Created new feedback for message {message_id}: {feedback}")
        
        conn.commit()
        conn.close()
        return feedback_id
    
    def get_feedback(self, message_id: int) -> Optional[Dict]:
        """Get feedback for a specific message."""
        conn = sqlite3.connect(self.db_path)
        conn.row_factory = sqlite3.Row
        cursor = conn.cursor()
        
        cursor.execute("""
            SELECT id, message_id, session_id, feedback, timestamp
            FROM message_feedback
            WHERE message_id = ?
            LIMIT 1
        """, (message_id,))
        
        row = cursor.fetchone()
        conn.close()
        
        if row:
            return {
                "id": row["id"],
                "message_id": row["message_id"],
                "session_id": row["session_id"],
                "feedback": row["feedback"],
                "timestamp": row["timestamp"]
            }
        return None

