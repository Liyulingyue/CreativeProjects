from fastapi import APIRouter

router = APIRouter(tags=["system"])

@router.get("/info")
async def get_system_info():
    """获取基础系统 CPU、内存和磁盘信息"""
    from ..services import SystemService
    return SystemService.get_system_info()

@router.get("/docker")
async def get_docker_info():
    """获取宿主机 Docker 容器状态"""
    from ..services import SystemService
    return SystemService.get_docker_containers()

@router.get("/processes")
async def get_process_list(limit: int = 50):
    """获取当前活跃进程列表"""
    from ..services import SystemService
    return SystemService.get_running_processes(limit=limit)

@router.post("/processes/{pid}/kill")
async def kill_process(pid: int):
    """结束指定进程"""
    from ..services import SystemService
    return SystemService.kill_process(pid)

@router.get("/network")
async def get_network_info():
    """获取网络接口和连接统计"""
    from ..services import SystemService
    return SystemService.get_network_info()

@router.get("/startup")
async def get_startup_stats():
    """获取负载和开机/运行相关的状态统计"""
    from ..services import SystemService
    return SystemService.get_startup_info()

@router.post("/docker/{container_id}/{action}")
async def container_action(container_id: str, action: str):
    """控制容器启动、停止、重启"""
    from ..services import SystemService
    if action not in ["start", "stop", "restart"]:
        return {"status": "error", "message": "Invalid action"}
    return SystemService.manage_docker_container(container_id, action)

@router.get("/startup/{name}/logs")
async def get_service_logs(name: str, lines: int = 100):
    """获取指定 systemd 服务的 journal 日志"""
    from ..services import SystemService
    return SystemService.get_service_logs(name, lines)

@router.post("/docker/auth")
async def docker_auth(body: dict):
    """提交 sudo 密码进行 Docker 认证"""
    import subprocess
    from ..services import _docker_authenticated
    password = body.get("password", "")
    _docker_authenticated["password"] = password
    result = subprocess.run(
        ["sudo", "-S", "-k", "docker", "info"],
        input=password + "\n", capture_output=True, text=True
    )
    if result.returncode != 0:
        _docker_authenticated["password"] = None
        return {"status": "error", "message": "密码错误或权限不足"}
    return {"status": "ok"}

@router.delete("/docker/auth")
async def docker_logout():
    """清除认证状态"""
    from ..services import _docker_authenticated
    _docker_authenticated["password"] = None
    return {"status": "ok"}
