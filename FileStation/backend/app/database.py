import sqlite3
from .core.config import settings

def get_db_connection():
    conn = sqlite3.connect(settings.DATABASE)
    return conn

def init_db():
    conn = get_db_connection()
    cursor = conn.cursor()
    cursor.execute('''
        CREATE TABLE IF NOT EXISTS files (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            filename TEXT NOT NULL,
            hash TEXT NOT NULL,
            size INTEGER,
            upload_time TEXT,
            comment TEXT
        )
    ''')
    cursor.execute('''
        CREATE TABLE IF NOT EXISTS folders (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            path TEXT NOT NULL UNIQUE,
            create_time TEXT
        )
    ''')
    conn.commit()
    conn.close()
