from fastapi import APIRouter, UploadFile, File, HTTPException, Form
from fastapi.responses import FileResponse
import os
import hashlib
import difflib
from .. import crud
from ..core.config import UPLOAD_DIR

router = APIRouter()

@router.post("/upload")
async def upload_file(
    file: UploadFile = File(...), 
    target_path: str = Form(None), 
    comment: str = Form("")
):
    final_filename = target_path if target_path else file.filename
    content = await file.read()
    file_hash = hashlib.sha256(content).hexdigest()
    
    crud.save_file_to_storage(content, file_hash)
    file_id = crud.record_file_upload(final_filename, file_hash, len(content), comment)
    
    return {"message": "File uploaded successfully", "file_id": file_id, "hash": file_hash, "path": final_filename}

@router.get("/files")
def list_files():
    return crud.get_all_files()

@router.get("/download/{file_id}")
def download_file(file_id: int):
    result = crud.get_file_metadata(file_id)
    if not result:
        raise HTTPException(status_code=404, detail="File not found")
    
    filename, file_hash = result
    file_path = os.path.join(UPLOAD_DIR, file_hash)
    if not os.path.exists(file_path):
        raise HTTPException(status_code=404, detail="File not found on disk")
    
    return FileResponse(file_path, media_type='application/octet-stream', filename=filename)

@router.get("/versions/{filename}")
def get_versions(filename: str):
    return crud.get_file_versions(filename)

@router.get("/diff/{filename}")
def get_diff(filename: str, v1: str, v2: str):
    path1 = os.path.join(UPLOAD_DIR, v1)
    path2 = os.path.join(UPLOAD_DIR, v2)
    
    if not os.path.exists(path1) or not os.path.exists(path2):
        raise HTTPException(status_code=404, detail="One or both versions not found")
    
    try:
        with open(path1, "r", encoding="utf-8") as f1, open(path2, "r", encoding="utf-8") as f2:
            lines1 = f1.readlines()
            lines2 = f2.readlines()
            
            diff = difflib.unified_diff(
                lines1, lines2, 
                fromfile=f"version_{v1[:8]}", 
                tofile=f"version_{v2[:8]}"
            )
            return {"diff": "".join(list(diff))}
    except UnicodeDecodeError:
        return {"diff": "[Binary file - Diff not supported]"}
    except Exception as e:
        return {"diff": f"Error calculating diff: {str(e)}"}

@router.get("/download/hash/{content_hash}")
def download_by_hash(content_hash: str, filename: str = "download"):
    file_path = os.path.join(UPLOAD_DIR, content_hash)
    if not os.path.exists(file_path):
        raise HTTPException(status_code=404, detail="File version not found")
    
    return FileResponse(file_path, media_type='application/octet-stream', filename=filename)

@router.delete("/files")
def delete_file(filename: str):
    crud.delete_file_record(filename)
    return {"message": f"File {filename} deleted"}

@router.post("/files/move")
def move_file(old_path: str = Form(...), new_path: str = Form(...), is_folder: bool = Form(False)):
    if is_folder:
        crud.move_folder_records(old_path, new_path)
    else:
        crud.move_file_record(old_path, new_path)
    return {"message": "Moved successfully"}
