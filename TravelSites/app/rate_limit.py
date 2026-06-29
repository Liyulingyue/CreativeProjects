"""
简单的内存级 rate limiter。

每 IP 滑动窗口：60s 内最多 N 次搜索。
不需要 Redis，单进程足够 demo 用。
"""
import time
from collections import defaultdict
from typing import Tuple


class RateLimiter:
    def __init__(self, max_requests: int = 30, window_seconds: int = 60):
        self.max = max_requests
        self.window = window_seconds
        self.requests: dict[str, list[float]] = defaultdict(list)

    def check(self, key: str) -> Tuple[bool, int]:
        """
        检查是否允许请求。
        Returns: (allowed, remaining_seconds_to_reset)
        """
        now = time.time()
        # 清理过期记录
        self.requests[key] = [
            t for t in self.requests[key] if now - t < self.window
        ]

        if len(self.requests[key]) >= self.max:
            oldest = self.requests[key][0]
            remaining = int(self.window - (now - oldest))
            return False, remaining

        self.requests[key].append(now)
        return True, 0


# 全局单例
search_limiter = RateLimiter(max_requests=30, window_seconds=60)
admin_limiter = RateLimiter(max_requests=60, window_seconds=60)