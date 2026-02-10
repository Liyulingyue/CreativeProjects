from fastapi import FastAPI, UploadFile, File, HTTPException, Form
from fastapi.responses import FileResponse
from fastapi.middleware.cors import CORSMiddleware
import os
import hashlib
import sqlite3
from datetime import datetime
import difflib

app = FastAPI(title="GittlyFileStation API", version="0.1.0")

# Add CORS middleware
app.add_middleware(
    CORSMiddleware,
    allow_origins=["*"],
    allow_credentials=True,
    allow_methods=["*"],
    allow_headers=["*"],
)

# Database setup
DATABASE = "gittly.db"
UPLOAD_DIR = "uploads"

os.makedirs(UPLOAD_DIR, exist_ok=True)

def init_db():
    conn = sqlite3.connect(DATABASE)
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

init_db()

@app.post("/upload")
async def upload_file(
    file: UploadFile = File(...), 
    target_path: str = Form(None), 
    comment: str = Form("")
):
    # Use target_path if provided, otherwise use original filename
    final_filename = target_path if target_path else file.filename
    
    # Calculate file hash
    content = await file.read()
    file_hash = hashlib.sha256(content).hexdigest()
    
    # Save file (Blob storage)
    file_path = os.path.join(UPLOAD_DIR, f"{file_hash}")
    if not os.path.exists(file_path):
        with open(file_path, "wb") as f:
            f.write(content)
    
    # Save to database (Logical file record)
    conn = sqlite3.connect(DATABASE)
    cursor = conn.cursor()
    
    # Check if this filename already exists to update or create new entry
    cursor.execute("SELECT id FROM files WHERE filename = ?", (final_filename,))
    existing = cursor.fetchone()
    
    now = datetime.now().isoformat()
    if existing:
        cursor.execute(
            "UPDATE files SET hash = ?, size = ?, upload_time = ?, comment = ? WHERE id = ?",
            (file_hash, len(content), now, comment, existing[0])
        )
        file_id = existing[0]
    else:
        cursor.execute(
            "INSERT INTO files (filename, hash, size, upload_time, comment) VALUES (?, ?, ?, ?, ?)",
            (final_filename, file_hash, len(content), now, comment)
        )
        file_id = cursor.lastrowid
    
    # Record version
    cursor.execute(
        "INSERT INTO file_versions (filename, version_hash, content_hash, upload_time, comment) VALUES (?, ?, ?, ?, ?)",
        (final_filename, file_hash, file_hash, now, comment)
    )
    conn.commit()
    conn.close()
    
    return {"message": "File uploaded successfully", "file_id": file_id, "hash": file_hash, "path": final_filename}

@app.get("/files")
def list_files():
    conn = sqlite3.connect(DATABASE)
    cursor = conn.cursor()
    cursor.execute("SELECT id, filename, hash, size, upload_time, comment FROM files ORDER BY upload_time DESC")
    files = cursor.fetchall()
    conn.close()
    return [{"id": f[0], "filename": f[1], "hash": f[2], "size": f[3], "upload_time": f[4], "comment": f[5]} for f in files]

@app.get("/download/{file_id}")
def download_file(file_id: int):
    conn = sqlite3.connect(DATABASE)
    cursor = conn.cursor()
    cursor.execute("SELECT filename, hash FROM files WHERE id = ?", (file_id,))
    result = cursor.fetchone()
    conn.close()
    if not result:
        raise HTTPException(status_code=404, detail="File not found")
    
    filename, file_hash = result
    file_path = os.path.join(UPLOAD_DIR, file_hash)
    if not os.path.exists(file_path):
        raise HTTPException(status_code=404, detail="File not found on disk")
    
    return FileResponse(file_path, media_type='application/octet-stream', filename=filename)

@app.get("/versions/{filename}")
def get_versions(filename: str):
    conn = sqlite3.connect(DATABASE)
    cursor = conn.cursor()
    cursor.execute("SELECT id, version_hash, content_hash, upload_time, comment FROM file_versions WHERE filename = ? ORDER BY upload_time DESC", (filename,))
    versions = cursor.fetchall()
    conn.close()
    return [{"id": v[0], "version_hash": v[1], "content_hash": v[2], "upload_time": v[3], "comment": v[4]} for v in versions]

@app.get("/diff/{filename}/{version1}/{version2}")
def get_diff(filename: str, version1: str, version2: str):
    # This is a simplified diff for text files
    # In real implementation, you'd load the actual file contents
    # For MVP, just return a placeholder
    return {"diff": "Diff functionality to be implemented for text files"}

@app.get("/")
def root():
    return {"message": "GittlyFileStation API"}