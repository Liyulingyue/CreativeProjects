import os

DATABASE = "filestation.db"
UPLOAD_DIR = "uploads"

# Storage Configuration
# Mode: "path" (Direct filesystem) or "cas" (Content-Addressed)
STORAGE_MODE = os.getenv("STORAGE_MODE", "path") 
USE_DATABASE = os.getenv("USE_DATABASE", "true").lower() == "true"

os.makedirs(UPLOAD_DIR, exist_ok=True)
