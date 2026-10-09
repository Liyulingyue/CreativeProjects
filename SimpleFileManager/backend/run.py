import os
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import uvicorn


def main():
    port = int(os.getenv("BACKEND_PORT", "8000"))
    # Reload spawns a supervisor + worker, which makes the parent pid useless for
    # stop/status scripts. Disable it when we were started by start_dev_all.sh.
    reload_enabled = os.getenv("BACKEND_RELOAD", "1") == "1"
    uvicorn.run(
        "app.main:app",
        host="0.0.0.0",
        port=port,
        reload=reload_enabled,
    )


if __name__ == "__main__":
    main()
