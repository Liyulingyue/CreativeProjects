from fastapi import APIRouter, UploadFile, File, HTTPException, Form
from fastapi.responses import FileResponse
import os
import hashlib
from .. import manager
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
    
    result = manager.upload_file(content, final_filename, comment)
    
    return {
        "message": "File uploaded successfully", 
        "file_id": result["id"], 
        "hash": result["hash"], 
        "path": final_filename
    }

@router.get("/files")
def list_files(prefix: str = ""):
    return manager.get_file_list(prefix)

@router.get("/download/{file_id_or_path:path}")
def download_file(file_id_or_path: str):
    # This is a bit flexible to handle both ID-based and Path-based downloads
    # For now, let's keep it simple: if it's numeric, try DB, else try Path
    file_path = ""
    display_name = ""
    
    if file_id_or_path.isdigit():
        from .. import crud
        result = crud.get_file_metadata(int(file_id_or_path))
        if result:
            file_path = os.path.join(UPLOAD_DIR, result[0])
            display_name = os.path.basename(result[0])
    
    if not file_path:
        # Fallback to direct path
        file_path = os.path.join(UPLOAD_DIR, file_id_or_path)
        display_name = os.path.basename(file_id_or_path)

    if not os.path.exists(file_path):
        raise HTTPException(status_code=404, detail="File not found")
    
    return FileResponse(file_path, media_type='application/octet-stream', filename=display_name)

@router.delete("/files")
def delete_file(filename: str):
    manager.delete_item(filename)
    return {"message": f"Item {filename} deleted"}

@router.post("/files/move")
def move_file(old_path: str = Form(...), new_path: str = Form(...), is_folder: bool = Form(False)):
    manager.move_item(old_path, new_path, is_folder)
    return {"message": "Moved successfully"}
