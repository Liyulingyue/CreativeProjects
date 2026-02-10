from .core.config import STORAGE_MODE, USE_DATABASE
from .storage.path_driver import PathStorageDriver
from .storage.cas_driver import CASStorageDriver
from . import crud

# Initialize Driver based on config
if STORAGE_MODE == "cas":
    storage_manager = CASStorageDriver()
else:
    storage_manager = PathStorageDriver() 

def get_file_list(prefix: str = ""):
    if USE_DATABASE:
        # Get flattened list from DB and filter by prefix in memory (current logic)
        # or we could optimize crud to take a prefix
        all_files = crud.get_all_files()
        folders = set()
        current_files = []
        
        norm_prefix = prefix + "/" if prefix and not prefix.endswith("/") else prefix
        
        for f in all_files:
            fname = f["filename"]
            if fname.startswith(norm_prefix):
                rel = fname[len(norm_prefix):]
                if "/" in rel:
                    folders.add(rel.split("/")[0])
                else:
                    current_files.append(f)
        return {"folders": sorted(list(folders)), "files": current_files}
    else:
        # Direct FS scan via driver
        return storage_manager.list_contents(prefix)

def upload_file(content: bytes, filename: str, comment: str):
    import hashlib
    file_hash = hashlib.sha256(content).hexdigest()
    
    # 1. Save to physical storage
    storage_manager.save_file(content, filename)
    
    # 2. Update DB if enabled
    file_id = None
    if USE_DATABASE:
        file_id = crud.record_file_upload(filename, file_hash, len(content), comment)
    
    return {"id": file_id, "hash": file_hash}

def delete_item(path: str):
    storage_manager.delete_item(path)
    if USE_DATABASE:
        crud.delete_file_record(path)

def move_item(old_path: str, new_path: str, is_folder: bool):
    storage_manager.move_item(old_path, new_path)
    if USE_DATABASE:
        if is_folder:
            crud.move_folder_records(old_path, new_path)
        else:
            crud.move_file_record(old_path, new_path)
