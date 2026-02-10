from fastapi import APIRouter, UploadFile, File, HTTPException, Form
from fastapi.responses import FileResponse
from typing import List, Optional
import os
import hashlib
from .. import manager
from ..core.config import settings

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
            file_path = os.path.join(settings.UPLOAD_DIR, result[0])
            display_name = os.path.basename(result[0])
    
    if not file_path:
        # Fallback to direct path
        file_path = os.path.join(settings.UPLOAD_DIR, file_id_or_path)
        display_name = os.path.basename(file_id_or_path)

    if not os.path.exists(file_path):
        raise HTTPException(status_code=404, detail="File not found")
    
    return FileResponse(file_path, media_type='application/octet-stream', filename=display_name)

@router.post("/files/create-folder")
def create_folder(path: str = Form(...)):
    return manager.create_folder(path)

@router.delete("/files")
def delete_file(filename: str):
    manager.delete_item(filename)
    return {"message": f"Item {filename} deleted"}

@router.post("/files/move-batch")
def move_files_batch(
    old_paths: List[str] = Form(...), 
    new_paths: List[str] = Form(...), 
    is_folders: List[bool] = Form(...)
):
    results = []
    for old_path, new_path, is_folder in zip(old_paths, new_paths, is_folders):
        try:
            manager.move_item(old_path, new_path, is_folder)
            results.append({"path": old_path, "status": "success"})
        except Exception as e:
            results.append({"path": old_path, "status": "error", "message": str(e)})
    return {"results": results}

@router.post("/files/move")
def move_file(old_path: str = Form(...), new_path: str = Form(...), is_folder: bool = Form(False)):
    try:
        manager.move_item(old_path, new_path, is_folder)
        return {"message": "Moved successfully"}
    except FileNotFoundError:
        raise HTTPException(status_code=404, detail=f"Source item not found: {old_path}")
    except Exception as e:
        import traceback
        traceback.print_exc()
        raise HTTPException(status_code=500, detail=str(e))
