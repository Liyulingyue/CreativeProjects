"""
FastAPI 依赖：用户认证。

用法：
  @router.get("/me")
  async def me(user: dict = Depends(require_user)):
      return {"username": user["username"]}

  @router.post("/admin/foo")
  async def foo(user: dict = Depends(require_admin)):
      ...
"""
from fastapi import Header, HTTPException, Depends
from typing import Optional

from src.auth import verify_session


async def get_current_user(
    authorization: Optional[str] = Header(None),
) -> Optional[dict]:
    """从 Authorization header 提取当前用户。未登录返回 None。"""
    if not authorization:
        return None
    token = authorization.replace("Bearer ", "").strip()
    if not token:
        return None
    return verify_session(token)


async def require_user(
    user: Optional[dict] = Depends(get_current_user),
) -> dict:
    """要求登录。"""
    if not user:
        raise HTTPException(status_code=401, detail="未登录或登录已过期")
    return user


async def require_admin(
    user: dict = Depends(require_user),
) -> dict:
    """要求 admin 角色。"""
    if user.get("role") != "admin":
        raise HTTPException(status_code=403, detail="需要管理员权限")
    return user