import os
import shutil
from datetime import datetime
from .database import get_db_connection
from .core.config import UPLOAD_DIR

def save_file_to_storage(content: bytes, relative_path: str):
    full_path = os.path.join(UPLOAD_DIR, relative_path)
    # Ensure nested directories exist
    os.makedirs(os.path.dirname(full_path), exist_ok=True)
    with open(full_path, "wb") as f:
        f.write(content)
    return full_path

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

def delete_file_record(filename: str):
    conn = get_db_connection()
    cursor = conn.cursor()
    # Delete from main files list (including sub-items if it's a folder)
    cursor.execute("DELETE FROM files WHERE filename = ? OR filename LIKE ? || '%'", (filename, filename + '/'))
    conn.commit()
    conn.close()

def move_file_record(old_name: str, new_name: str):
    conn = get_db_connection()
    cursor = conn.cursor()
    # Update main files list
    cursor.execute("UPDATE files SET filename = ? WHERE filename = ?", (new_name, old_name))
    conn.commit()
    conn.close()

def move_folder_records(old_prefix: str, new_prefix: str):
    conn = get_db_connection()
    cursor = conn.cursor()
    # Ensure prefixes end with /
    op = old_prefix if old_prefix.endswith('/') else old_prefix + '/'
    np = new_prefix if new_prefix.endswith('/') else new_prefix + '/'
    
    # Update files mapping in DB
    cursor.execute(
        "UPDATE files SET filename = ? || SUBSTR(filename, ?) WHERE filename LIKE ? || '%'",
        (np, len(op) + 1, op)
    )
    conn.commit()
    conn.close()
