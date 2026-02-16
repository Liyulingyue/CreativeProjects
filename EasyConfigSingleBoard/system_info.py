import psutil
import socket
import subprocess

def get_cpu_info():
    return {
        "percent": psutil.cpu_percent(interval=1),
        "count": psutil.cpu_count()
    }

def get_memory_info():
    mem = psutil.virtual_memory()
    return {
        "total": round(mem.total / (1024**3), 2),
        "available": round(mem.available / (1024**3), 2),
        "percent": mem.percent
    }

def get_ip_addresses():
    interfaces = {}
    for interface, snics in psutil.net_if_addrs().items():
        for snic in snics:
            if snic.family == socket.AF_INET:
                interfaces[interface] = snic.address
    return interfaces

def get_wifi_networks():
    try:
        # Use nmcli to scan wifi
        result = subprocess.check_output(["nmcli", "-t", "-f", "SSID,SIGNAL,SECURITY", "dev", "wifi", "list"], encoding="utf-8")
        networks = []
        for line in result.strip().split("\n"):
            if not line: continue
            parts = line.split(":")
            if len(parts) >= 3:
                networks.append({
                    "ssid": parts[0],
                    "signal": parts[1],
                    "security": parts[2]
                })
        return networks
    except Exception as e:
        return []

if __name__ == "__main__":
    print("CPU:", get_cpu_info())
    print("Memory:", get_memory_info())
    print("IPs:", get_ip_addresses())
    print("WIFI:", get_wifi_networks())
