#!/usr/bin/env python
"""Quick diagnostic script to check what's in the database."""
import sqlite3
from pathlib import Path

db_path = Path(__file__).parent / "data" / "chat_history.db"

if not db_path.exists():
    print(f"Database not found at {db_path}")
    exit(1)

conn = sqlite3.connect(db_path)
conn.row_factory = sqlite3.Row
cursor = conn.cursor()

# Get all unique sessions
cursor.execute("SELECT DISTINCT session_id FROM chat_messages ORDER BY session_id")
sessions = cursor.fetchall()

print(f"Found {len(sessions)} unique sessions:\n")

for session_row in sessions:
    session_id = session_row['session_id']
    
    # Count messages
    cursor.execute("SELECT COUNT(*) as cnt FROM chat_messages WHERE session_id = ?", (session_id,))
    count = cursor.fetchone()['cnt']
    
    # Get latest timestamp
    cursor.execute("""
        SELECT timestamp FROM chat_messages 
        WHERE session_id = ? 
        ORDER BY timestamp DESC 
        LIMIT 1
    """, (session_id,))
    latest = cursor.fetchone()
    
    print(f"Session: {session_id}")
    print(f"  Messages: {count}")
    print(f"  Latest: {latest['timestamp'] if latest else 'N/A'}")
    
    # Show message roles
    cursor.execute("""
        SELECT role, COUNT(*) as cnt 
        FROM chat_messages 
        WHERE session_id = ? 
        GROUP BY role
    """, (session_id,))
    roles = cursor.fetchall()
    
    for role_row in roles:
        print(f"    {role_row['role']}: {role_row['cnt']}")
    
    # Show one sample message
    cursor.execute("""
        SELECT role, content FROM chat_messages 
        WHERE session_id = ? 
        LIMIT 1
    """, (session_id,))
    sample = cursor.fetchone()
    if sample:
        content_preview = (sample['content'][:60] + "...") if len(sample['content']) > 60 else sample['content']
        print(f"  Sample: [{sample['role']}] {content_preview}")
    
    print()

conn.close()
