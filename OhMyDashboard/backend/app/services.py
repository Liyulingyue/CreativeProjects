import psutil
import docker
import platform
import os
from datetime import datetime

class SystemService:
    @staticmethod
    def get_system_info():
        # CPU 逻辑核心与物理核心
        cpu_count_logical = psutil.cpu_count(logical=True)
        cpu_count_physical = psutil.cpu_count(logical=False)
        
        # 内存
        mem = psutil.virtual_memory()
        swap = psutil.swap_memory()
        
        # 磁盘 (只看根目录)
        disk_usage = psutil.disk_usage('/')._asdict()
        
        # 网络流量
        net_io = psutil.net_io_counters()._asdict()

        return {
            "os": platform.system(),
            "os_release": platform.release(),
            "cpu_count": cpu_count_logical,
            "cpu_physical": cpu_count_physical,
            "cpu_freq": psutil.cpu_freq()._asdict() if psutil.cpu_freq() else None,
            "cpu_percent": psutil.cpu_percent(interval=None),
            "memory": {
                "total": mem.total,
                "available": mem.available,
                "used": mem.used,
                "percent": mem.percent,
                "swap_total": swap.total,
                "swap_used": swap.used,
                "swap_percent": swap.percent
            },
            "disk": disk_usage,
            "network": net_io,
            "boot_time": datetime.fromtimestamp(psutil.boot_time()).isoformat(),
            "uptime_seconds": int(datetime.now().timestamp() - psutil.boot_time())
        }

    @staticmethod
    def get_docker_containers():
        try:
            client = docker.from_env()
            containers = client.containers.list(all=True)
            return [
                {
                    "id": c.short_id,
                    "name": c.name,
                    "status": c.status,
                    "image": c.image.tags[0] if c.image.tags else "unknown",
                    "state": c.attrs.get('State', {}),
                    "ports": c.ports,
                    "created": c.attrs.get('Created')
                } for c in containers
            ]
        except Exception as e:
            return {"error": f"Docker not accessible: {str(e)}"}

    @staticmethod
    def manage_docker_container(container_id: str, action: str):
        try:
            client = docker.from_env()
            container = client.containers.get(container_id)
            if action == "start":
                container.start()
            elif action == "stop":
                container.stop()
            elif action == "restart":
                container.restart()
            return {"status": "success", "action": action, "id": container_id}
        except Exception as e:
            return {"status": "error", "message": str(e)}

    @staticmethod
    def get_running_processes(limit=50):
        processes = []
        # 性能优化：只获取必要的字段
        attrs = ['pid', 'name', 'username', 'cpu_percent', 'memory_percent', 'status']
        for proc in psutil.process_iter(attrs):
            try:
                pinfo = proc.info
                processes.append(pinfo)
            except (psutil.NoSuchProcess, psutil.AccessDenied, psutil.ZombieProcess):
                pass
        
        # 按 CPU 占用排序
        processes.sort(key=lambda x: x.get('cpu_percent', 0), reverse=True)
        return processes[:limit]

    @staticmethod
    def get_startup_info():
        # 获取系统负载、活跃用户等
        return {
            "load_avg": os.getloadavg() if hasattr(os, 'getloadavg') else None,
            "users": [u._asdict() for u in psutil.users()],
        }
