import os
import sys
import shutil
import tempfile
from pathlib import Path

import pytest

_tmp = tempfile.mkdtemp(prefix="sfm_test_")
os.environ["STORAGE_PATH"] = _tmp
os.environ["LLM_API_KEY"] = ""
os.environ["LLM_BASE_URL"] = "http://localhost:9999/v1"
os.environ["LLM_MODEL"] = "test-model"
os.environ["EMBEDDING_API_KEY"] = ""
os.environ["EMBEDDING_BASE_URL"] = "http://localhost:9999/v1"
os.environ["EMBEDDING_MODEL"] = "test-embed"
os.environ["EMBEDDING_DIM"] = "128"
os.environ["INDEX_INTERVAL"] = "9999"

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

from fastapi.testclient import TestClient
from app.main import app
from app.deps import state, get_storage_root

client = TestClient(app)


@pytest.fixture(autouse=True)
def reset_storage():
    root = get_storage_root()
    for child in root.iterdir():
        if child.name == ".simplefilemanager":
            continue
        if child.is_dir():
            shutil.rmtree(child)
        else:
            child.unlink()
    yield


def create_test_file(relative_path: str, content: str = "hello"):
    root = get_storage_root()
    fp = root / relative_path
    fp.parent.mkdir(parents=True, exist_ok=True)
    fp.write_text(content, encoding="utf-8")
    return str(fp)
