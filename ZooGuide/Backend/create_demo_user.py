"""Create demo account for testing. Run once:
    source Backend/.venv/bin/activate
    python Backend/create_demo_user.py
"""
import sys
from pathlib import Path
sys.path.insert(0, str(Path(__file__).resolve().parent))

from app import auth, db


def main():
    db.init_db()
    if db.find_user_by_username("demo"):
        print("demo account already exists")
        return
    uid = db.create_user("demo", auth.hash_password("demo1234"), "演示用户")
    print(f"✓ Created demo user (id={uid})")
    print("  username: demo")
    print("  password: demo1234")


if __name__ == "__main__":
    main()