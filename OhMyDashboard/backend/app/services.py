import psutil
import docker
import platform
import os
from datetime import datetime

class SystemService:
    @staticmethod
    def get_system_info():
        return {
            "os": platform.system(),
            "os_release": platform.release(),
            "cpu_count": psutil.cpu_count(logical=True),
            "cpu_freq": psutil.cpu_freq()._asdict() if psutil.cpu_freq() else None,
            "memory": psutil.virtual_memory()._asdict(),
            "disk": [d._asdict() for d in psutil.disk_partitions() if 'loop' not in d.device],
            "boot_time": datetime.fromtimestamp(psutil.boot_time()).isoformat()
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
                    "ports": c.ports
                } for c in containers
            ]
        except Exception as e:
            return {"error": f"Docker not accessible: {str(e)}"}

    @staticmethod
    def get_running_processes(limit=50):
        processes = []
        for proc in psutil.process_iter(['pid', 'name', 'username', 'cpu_percent', 'memory_percent', 'create_time']):
            try:
                pinfo = proc.info
                pinfo['create_time'] = datetime.fromtimestamp(pinfo['create_time']).isoformat()
                processes.append(pinfo)
            except (psutil.NoSuchProcess, psutil.AccessDenied, psutil.ZombieProcess):
                pass
        
        # 按 CPU 占用排序并取前 N 个
        processes.sort(key=lambda x: x.get('cpu_percent', 0), reverse=True)
        return processes[:limit]

    @staticmethod
    def get_startup_info():
        # 获取系统负载、活跃用户等
        return {
            "load_avg": os.getloadavg() if hasattr(os, 'getloadavg') else None,
            "users": [u._asdict() for u in psutil.users()],
        }
