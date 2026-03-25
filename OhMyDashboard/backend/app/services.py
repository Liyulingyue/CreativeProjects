import psutil
import docker
import platform
import os
import socket
import subprocess
from datetime import datetime

_docker_authenticated: dict = {}

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
    def _check_permission_error(error: str) -> bool:
        return "Permission denied" in error or "PermissionError(13" in error

    @staticmethod
    def _run_docker_cmd(args: list) -> subprocess.CompletedProcess:
        pwd = _docker_authenticated.get("password")
        if pwd:
            return subprocess.run(
                ["sudo", "-S", "-k"] + args,
                input=pwd + "\n", capture_output=True, text=True
            )
        return subprocess.run(args, capture_output=True, text=True)

    @staticmethod
    def get_docker_containers():
        try:
            res = SystemService._run_docker_cmd(["docker", "ps", "-a", "--format", "{{.ID}}|{{.Names}}|{{.Status}}|{{.Image}}|{{.Ports}}|{{.CreatedAt}}|{{.State}}"])
            if res.returncode != 0:
                err = res.stderr.strip()
                if "permission denied" in err.lower() or "cannot connect" in err.lower():
                    return {"error": "Docker not accessible: Permission denied", "need_auth": True}
                return {"error": f"Docker error: {err}"}
            containers = []
            for line in res.stdout.strip().splitlines():
                parts = line.split("|")
                if len(parts) >= 6:
                    containers.append({
                        "id": parts[0],
                        "name": parts[1],
                        "status": "running" if "Up" in parts[2] else "exited",
                        "image": parts[3],
                        "ports": parts[4],
                        "created": parts[5],
                        "state": {"Status": parts[6].lower()} if len(parts) > 6 else {},
                    })
            return containers
        except Exception as e:
            return {"error": f"Docker error: {str(e)}"}

    @staticmethod
    def manage_docker_container(container_id: str, action: str):
        res = SystemService._run_docker_cmd(["docker", action, container_id])
        if res.returncode != 0:
            err = res.stderr.strip()
            if "permission denied" in err.lower():
                return {"status": "error", "message": "Permission denied", "need_auth": True}
            return {"status": "error", "message": err}
        return {"status": "success", "action": action, "id": container_id}

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
        """
        try:
            import subprocess
            PROPERTY = 'Description,ActiveState,SubState,LoadState,MainPID,MemoryCurrent,CPU'

            res = subprocess.run(
                ['systemctl', 'list-unit-files', '--type=service', '--no-pager', '--no-legend'],
                capture_output=True, text=True
            )
            items = []
            for line in res.stdout.splitlines():
                parts = line.split(None, 2)
                if len(parts) >= 2 and '.service' in parts[0]:
                    name = parts[0]
                    status = parts[1]
                    vendor_preset = parts[2] if len(parts) > 2 else ''

                    show_res = subprocess.run(
                        ['systemctl', 'show', name, f'--property={PROPERTY}', '--no-pager'],
                        capture_output=True, text=True
                    )
                    info = {}
                    for line in show_res.stdout.splitlines():
                        if '=' in line:
                            k, v = line.split('=', 1)
                            info[k] = v if v else '-'

                    items.append({
                        "name": name,
                        "status": status,
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
    def get_service_logs(name: str, lines: int = 100):
        """
        获取指定 systemd 服务的 journal 日志
        """
        try:
            import subprocess
            res = subprocess.run(
                ['journalctl', '-u', name, '-n', str(lines), '--no-pager', '--reverse'],
                capture_output=True, text=True
            )
            log_lines = res.stdout.splitlines() if res.stdout else res.stderr.splitlines()
            return {"name": name, "logs": log_lines}
        except Exception as e:
            return {"name": name, "logs": [], "error": str(e)}

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
        tcp_conns = [c for c in connections if c.type == socket.SOCK_STREAM]
        udp_conns = [c for c in connections if c.type == socket.SOCK_DGRAM]

        listening_ports = {}
        for c in connections:
            if c.status == 'LISTEN':
                laddr = c.laddr
                port = laddr.port if laddr else 0
                key = f"{c.family.name}_{port}"
                if key not in listening_ports:
                    pid_name = ''
                    if c.pid:
                        try:
                            pid_name = psutil.Process(c.pid).name()
                        except Exception:
                            pid_name = str(c.pid)
                    listening_ports[key] = {
                        "port": port,
                        "protocol": "TCP" if c.type == socket.SOCK_STREAM else "UDP",
                        "pid": c.pid,
                        "process": pid_name,
                        "address": laddr.ip if laddr else '0.0.0.0',
                    }

        active_connections = []
        for c in connections:
            laddr = c.laddr
            raddr = c.raddr
            if c.status not in ('LISTEN', 'CLOSED') or raddr:
                active_connections.append({
                    "protocol": "TCP" if c.type == socket.SOCK_STREAM else "UDP",
                    "laddr": f"{laddr.ip}:{laddr.port}" if laddr else "-",
                    "raddr": f"{raddr.ip}:{raddr.port}" if raddr else "-",
                    "status": c.status,
                    "pid": c.pid,
                })

        return {
            "total": net_io,
            "interfaces": net_if,
            "connections": {
                "total": len(connections),
                "tcp": len(tcp_conns),
                "udp": len(udp_conns)
            },
            "listening_ports": list(listening_ports.values()),
            "active_connections": active_connections,
        }
