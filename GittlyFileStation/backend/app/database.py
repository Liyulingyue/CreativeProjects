import sqlite3
from .core.config import DATABASE

def get_db_connection():
    conn = sqlite3.connect(DATABASE)
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
        CREATE TABLE IF NOT EXISTS file_versions (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            filename TEXT NOT NULL,
            version_hash TEXT NOT NULL,
            content_hash TEXT NOT NULL,
            upload_time TEXT,
            comment TEXT
        )
    ''')
    conn.commit()
    conn.close()
