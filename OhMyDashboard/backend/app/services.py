import psutil
import docker
import platform
import os
import socket
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
        attrs = ['pid', 'name', 'username', 'cpu_percent', 'memory_percent', 'status', 'create_time', 'num_threads']
        for proc in psutil.process_iter(attrs):
            try:
                pinfo = proc.info
                if pinfo.get('create_time'):
                    from datetime import datetime
                    pinfo['create_time'] = datetime.fromtimestamp(pinfo['create_time']).isoformat()
                processes.append(pinfo)
            except (psutil.NoSuchProcess, psutil.AccessDenied, psutil.ZombieProcess):
                pass
        
        processes.sort(key=lambda x: x.get('cpu_percent', 0), reverse=True)
        return processes[:limit]

    @staticmethod
    def kill_process(pid: int):
        try:
            p = psutil.Process(pid)
            p.terminate()
            return {"status": "success", "pid": pid, "action": "terminate"}
        except psutil.NoSuchProcess:
            return {"status": "error", "message": f"进程 {pid} 不存在"}
        except psutil.AccessDenied:
            return {"status": "error", "message": f"无权限结束进程 {pid}"}
        except Exception as e:
            return {"status": "error", "message": str(e)}

    @staticmethod
    def get_startup_items():
        """
        获取 Linux 开机启动项 (基于 systemd)
        优化：批量查询服务详情，避免逐个调用 systemctl show
        """
        try:
            import subprocess
            BATCH_SIZE = 50
            PROPERTY = 'Description,ActiveState,SubState,LoadState,MainPID,MemoryCurrent,CPU'

            res = subprocess.run(
                ['systemctl', 'list-unit-files', '--type=service', '--no-pager', '--no-legend'],
                capture_output=True, text=True
            )
            service_map = {}
            for line in res.stdout.splitlines():
                parts = line.split(None, 2)
                if len(parts) >= 2 and '.service' in parts[0]:
                    service_map[parts[0]] = (parts[1], parts[2] if len(parts) > 2 else '')

            names = list(service_map.keys())
            all_info = {}

            for i in range(0, len(names), BATCH_SIZE):
                batch = names[i:i + BATCH_SIZE]
                show_res = subprocess.run(
                    ['systemctl', 'show'] + batch + [f'--property={PROPERTY}', '--no-pager'],
                    capture_output=True, text=True
                )
                current_name = ''
                current = {}
                for line in show_res.stdout.splitlines():
                    if line.startswith('# Name='):
                        if current_name:
                            all_info[current_name] = current
                        current_name = line.split('=', 1)[1].strip()
                        current = {}
                    elif '=' in line:
                        k, v = line.split('=', 1)
                        current[k] = v if v else '-'
                if current_name:
                    all_info[current_name] = current

            items = []
            for name, (state, vendor_preset) in service_map.items():
                info = all_info.get(name, {})
                items.append({
                    "name": name,
                    "status": state,
                    "vendor_preset": vendor_preset,
                    "description": info.get('Description', ''),
                    "active_state": info.get('ActiveState', ''),
                    "sub_state": info.get('SubState', ''),
                    "load_state": info.get('LoadState', ''),
                    "main_pid": info.get('MainPID', '-'),
                    "memory_current": info.get('MemoryCurrent', '-'),
                    "cpu": info.get('CPU', '-'),
                })
            return items
        except Exception as e:
            return {"error": str(e)}

    @staticmethod
    def get_startup_info():
        return {
            "load_avg": os.getloadavg() if hasattr(os, 'getloadavg') else None,
            "startup_items": SystemService.get_startup_items(),
            "users": [u._asdict() for u in psutil.users()]
        }

    @staticmethod
    def get_network_info():
        net_io = psutil.net_io_counters()._asdict()
        net_if = {}
        if_stats = {i: s for i, s in psutil.net_if_stats().items()}
        try:
            per_nic = psutil.net_io_counters_per_nic()
        except Exception:
            per_nic = {}
        for iface, addrs in psutil.net_if_addrs().items():
            io = {}
            if iface in per_nic:
                io = per_nic[iface]._asdict()
            stats = if_stats.get(iface)
            net_if[iface] = {
                "address": [a.address for a in addrs if a.family.name == 'AF_INET'],
                "mac": [a.address for a in addrs if a.family.name == 'AF_LINK'],
                "mtu": stats.mtu if stats else 0,
                "is_up": stats.isup if stats else False,
                "bytes_sent": io.get("bytes_sent", 0),
                "bytes_recv": io.get("bytes_recv", 0),
                "packets_sent": io.get("packets_sent", 0),
                "packets_recv": io.get("packets_recv", 0),
                "errin": io.get("errin", 0),
                "errout": io.get("errout", 0),
            }
        connections = psutil.net_connections()
        tcp = len([c for c in connections if c.type == socket.SOCK_STREAM])
        udp = len([c for c in connections if c.type == socket.SOCK_DGRAM])
        return {
            "total": net_io,
            "interfaces": net_if,
            "connections": {"total": len(connections), "tcp": tcp, "udp": udp}
        }
