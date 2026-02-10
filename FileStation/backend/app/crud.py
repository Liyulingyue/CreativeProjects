import os
import hashlib
from datetime import datetime
from .database import get_db_connection
from .core.config import UPLOAD_DIR

def save_file_to_storage(content: bytes, file_hash: str):
    file_path = os.path.join(UPLOAD_DIR, file_hash)
    if not os.path.exists(file_path):
        with open(file_path, "wb") as f:
            f.write(content)
    return file_path

def record_file_upload(filename: str, file_hash: str, size: int, comment: str):
    conn = get_db_connection()
    cursor = conn.cursor()
    
    cursor.execute("SELECT id FROM files WHERE filename = ?", (filename,))
    existing = cursor.fetchone()
    
    now = datetime.now().isoformat()
    if existing:
        cursor.execute(
            "UPDATE files SET hash = ?, size = ?, upload_time = ?, comment = ? WHERE id = ?",
            (file_hash, size, now, comment, existing[0])
        )
        file_id = existing[0]
    else:
        cursor.execute(
            "INSERT INTO files (filename, hash, size, upload_time, comment) VALUES (?, ?, ?, ?, ?)",
            (filename, file_hash, size, now, comment)
        )
        file_id = cursor.lastrowid
    
    # Record version
    cursor.execute(
        "INSERT INTO file_versions (filename, version_hash, content_hash, upload_time, comment) VALUES (?, ?, ?, ?, ?)",
        (filename, file_hash, file_hash, now, comment)
    )
    conn.commit()
    conn.close()
    return file_id

def get_all_files():
    conn = get_db_connection()
    cursor = conn.cursor()
    cursor.execute("SELECT id, filename, hash, size, upload_time, comment FROM files ORDER BY upload_time DESC")
    files = cursor.fetchall()
    conn.close()
    return [{"id": f[0], "filename": f[1], "hash": f[2], "size": f[3], "upload_time": f[4], "comment": f[5]} for f in files]

def get_file_metadata(file_id: int):
    conn = get_db_connection()
    cursor = conn.cursor()
    cursor.execute("SELECT filename, hash FROM files WHERE id = ?", (file_id,))
    result = cursor.fetchone()
    conn.close()
    return result

def get_file_versions(filename: str):
    conn = get_db_connection()
    cursor = conn.cursor()
    cursor.execute("SELECT id, version_hash, content_hash, upload_time, comment FROM file_versions WHERE filename = ? ORDER BY upload_time DESC", (filename,))
    versions = cursor.fetchall()
    conn.close()
    return [{"id": v[0], "version_hash": v[1], "content_hash": v[2], "upload_time": v[3], "comment": v[4]} for v in versions]

def delete_file_record(filename: str):
    conn = get_db_connection()
    cursor = conn.cursor()
    # Delete from main files list
    cursor.execute("DELETE FROM files WHERE filename = ?", (filename,))
    # Delete all versions
    cursor.execute("DELETE FROM file_versions WHERE filename = ?", (filename,))
    conn.commit()
    conn.close()

def move_file_record(old_name: str, new_name: str):
    conn = get_db_connection()
    cursor = conn.cursor()
    # Update main files list
    cursor.execute("UPDATE files SET filename = ? WHERE filename = ?", (new_name, old_name))
    # Update versions
    cursor.execute("UPDATE file_versions SET filename = ? WHERE filename = ?", (new_name, old_name))
    conn.commit()
    conn.close()

def move_folder_records(old_prefix: str, new_prefix: str):
    conn = get_db_connection()
    cursor = conn.cursor()
    # Ensure prefixes end with /
    op = old_prefix if old_prefix.endswith('/') else old_prefix + '/'
    np = new_prefix if new_prefix.endswith('/') else new_prefix + '/'
    
    # Update files (SQLite || is concatenation)
    cursor.execute(
        "UPDATE files SET filename = ? || SUBSTR(filename, ?) WHERE filename LIKE ? || '%'",
        (np, len(op) + 1, op)
    )
    # Update versions
    cursor.execute(
        "UPDATE file_versions SET filename = ? || SUBSTR(filename, ?) WHERE filename LIKE ? || '%'",
        (np, len(op) + 1, op)
    )
    conn.commit()
    conn.close()
