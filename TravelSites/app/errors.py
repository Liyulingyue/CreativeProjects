"""
统一错误处理与日志。

原则：
- 内部异常转友好的中文错误
- API 错误统一格式: {"error": "描述", "code": "错误代码"}
- 不泄露技术细节
"""
import logging
import traceback
from functools import wraps
from typing import Any, Callable

from fastapi import Request
from fastapi.responses import JSONResponse


# 配置日志（只显示 WARNING 及以上，避免 httpx 刷屏）
logging.basicConfig(
    level=logging.WARNING,
    format="%(asctime)s [%(levelname)s] %(name)s: %(message)s",
    datefmt="%H:%M:%S",
)
logging.getLogger("httpx").setLevel(logging.WARNING)
logging.getLogger("httpcore").setLevel(logging.WARNING)
logger = logging.getLogger("travelsites")


ERROR_CODES = {
    "INVALID_DATE": "日期格式错误，应为 YYYY-MM-DD",
    "DATE_RANGE_INVALID": "返回日期不能早于出发日期",
    "NO_RESULTS": "未找到匹配的行程",
    "CITY_NOT_FOUND": "城市不在系统中",
    "LLM_TIMEOUT": "AI 服务暂时繁忙，请稍后重试",
    "POI_DISABLED": "POI 数据源未启用",
    "INTERNAL": "服务异常，请稍后重试",
}


def api_error(code: str, status: int = 400, detail: str = None):
    """返回统一格式错误响应。"""
    message = detail or ERROR_CODES.get(code, code)
    return JSONResponse(
        status_code=status,
        content={"error": code, "message": message}
    )


def log_request(endpoint: str):
    """装饰器：记录 API 请求和异常。"""
    def decorator(func: Callable) -> Callable:
        @wraps(func)
        async def wrapper(*args, **kwargs):
            try:
                return await func(*args, **kwargs)
            except Exception as e:
                logger.error(f"[{endpoint}] {type(e).__name__}: {e}")
                logger.debug(traceback.format_exc())
                return api_error("INTERNAL", 500)
        return wrapper
    return decorator


def setup_exception_handlers(app):
    """注册全局异常处理器。"""
    @app.exception_handler(ValueError)
    async def value_error_handler(request: Request, exc: ValueError):
        logger.warning(f"[ValueError] {request.url.path}: {exc}")
        return api_error("INVALID_DATE" if "date" in str(exc).lower() else "INTERNAL", 400, str(exc))

    @app.exception_handler(Exception)
    async def general_exception_handler(request: Request, exc: Exception):
        logger.error(f"[Exception] {request.url.path}: {type(exc).__name__}: {exc}")
        logger.debug(traceback.format_exc())
        return api_error("INTERNAL", 500)