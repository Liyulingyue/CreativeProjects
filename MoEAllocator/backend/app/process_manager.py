import json
import os
import subprocess
import signal
import threading
from dataclasses import dataclass, field
from pathlib import Path
from typing import Optional
import httpx
import urllib.request

urllib.request.getproxies = lambda: {}


class ProcessInfo:
    def __init__(self, node_type: str, node_id: str, port: int, tcp_port: Optional[int], pid: int, process: subprocess.Popen, log_file: str):
        self.node_type = node_type
        self.node_id = node_id
        self.port = port
        self.tcp_port = tcp_port
        self.pid = pid
        self.process = process
        self.log_file = log_file
        self.http_port = port
        self.base_url: Optional[str] = None
        self.is_remote = False


class ProcessManager:
    _instance = None
    _lock = threading.Lock()

    def __new__(cls):
        if cls._instance is None:
            with cls._lock:
                if cls._instance is None:
                    cls._instance = super().__new__(cls)
                    cls._instance._processes = {}
                    cls._instance._remote_nodes = {}
                    cls._instance._base_dir = Path(__file__).parent.parent.parent
        return cls._instance

    def get_python_env(self, python_env: str = 'venv', custom_python: Optional[str] = None) -> tuple[str, dict]:
        base = python_env
        env = {k: v for k, v in os.environ.items() if 'proxy' not in k.lower()}
        if base == 'system':
            python_path = 'python3'
        elif base == 'custom':
            python_path = custom_python or (self._base_dir / '.venv' / 'bin' / 'python')
        else:
            venv_python = self._base_dir / '.venv' / 'bin' / 'python'
            venv_bin = self._base_dir / '.venv' / 'bin'
            python_path = str(venv_python)
            env['PATH'] = str(venv_bin) + ':' + os.environ.get('PATH', '')
        env['_PYTHON'] = str(python_path)
        return str(python_path), env

    def get_detected_envs(self) -> dict:
        venv_python = self._base_dir / '.venv' / 'bin' / 'python'
        detected = {}
        if venv_python.exists():
            detected['venv'] = str(venv_python)
        for name in ('python3', 'python'):
            import shutil
            p = shutil.which(name)
            if p:
                detected['system'] = p
                break
        return detected

    def _find_free_port(self, start: int = 5000) -> int:
        import socket
        for port in range(start, start + 1000):
            try:
                with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
                    s.bind(('127.0.0.1', port))
                    return port
            except OSError:
                continue
        return start

    def _get_log_path(self, node_type: str, node_id: str) -> str:
        log_dir = self._base_dir / 'logs'
        log_dir.mkdir(exist_ok=True)
        return str(log_dir / f'{node_type}_{node_id}.log')

    def start_master(self, node_id: str, manifest_path: str, http_port: Optional[int] = None,
                     host: str = '127.0.0.1',
                     expert_ids: Optional[str] = None, python_env: str = 'venv',
                     custom_python: Optional[str] = None) -> dict:
        if node_id in self._processes:
            raise ValueError(f"Node '{node_id}' already exists")

        port = http_port or self._find_free_port(5000)
        log_path = self._get_log_path('master', node_id)
        _, env = self.get_python_env(python_env, custom_python)

        python_path, _ = self.get_python_env(python_env, custom_python)
        cmd = [
            python_path, '-m', 'src.nexus.master',
            '--manifest', manifest_path,
            '--port', str(port),
            '--host', host,
        ]
        if expert_ids:
            cmd += ['--experts', expert_ids]

        process = subprocess.Popen(
            cmd,
            cwd=str(self._base_dir),
            env=env,
            stdout=open(log_path, 'w'),
            stderr=subprocess.STDOUT,
        )

        self._processes[node_id] = ProcessInfo(
            node_type='master',
            node_id=node_id,
            port=port,
            tcp_port=None,
            pid=process.pid,
            process=process,
            log_file=log_path,
        )

        import time
        time.sleep(2)

        return {
            'node_id': node_id,
            'node_type': 'master',
            'http_port': port,
            'pid': process.pid,
            'log_file': log_path,
        }

    def start_worker(self, node_id: str, http_port: Optional[int] = None, tcp_port: Optional[int] = None,
                     host: str = '127.0.0.1',
                     experts_dir: Optional[str] = None, expert_ids: Optional[str] = None,
                     master_url: Optional[str] = None, python_env: str = 'venv',
                     custom_python: Optional[str] = None) -> dict:
        if node_id in self._processes:
            raise ValueError(f"Node '{node_id}' already exists")

        port = http_port or self._find_free_port(8000)
        tcp = tcp_port or self._find_free_port(9000)
        log_path = self._get_log_path('worker', node_id)
        python_path, env = self.get_python_env(python_env, custom_python)

        cmd = [
            python_path, '-m', 'src.nexus.worker',
            '--id', node_id,
            '--http-port', str(port),
            '--tcp-port', str(tcp),
            '--host', host,
        ]
        if experts_dir:
            cmd += ['--experts-dir', experts_dir]
        if expert_ids:
            cmd += ['--expert-ids', ','.join(str(x) for x in expert_ids)]
        if master_url:
            cmd += ['--master', f'{master_url}/workers']

        process = subprocess.Popen(
            cmd,
            cwd=str(self._base_dir),
            env=env,
            stdout=open(log_path, 'w'),
            stderr=subprocess.STDOUT,
        )

        self._processes[node_id] = ProcessInfo(
            node_type='worker',
            node_id=node_id,
            port=port,
            tcp_port=tcp,
            pid=process.pid,
            process=process,
            log_file=log_path,
        )

        import time
        time.sleep(2)

        return {
            'node_id': node_id,
            'node_type': 'worker',
            'http_port': port,
            'tcp_port': tcp,
            'pid': process.pid,
            'log_file': log_path,
        }

    def add_remote_node(self, node_id: str, node_type: str, base_url: str, tcp_port: Optional[int] = None) -> dict:
        if node_id in self._processes or node_id in self._remote_nodes:
            raise ValueError(f"Node '{node_id}' already exists")
        if node_type not in ('master', 'worker'):
            raise ValueError("node_type must be 'master' or 'worker'")
        self._remote_nodes[node_id] = {
            'node_type': node_type,
            'node_id': node_id,
            'base_url': base_url.rstrip('/'),
            'http_port': self._extract_port(base_url),
            'tcp_port': tcp_port,
        }
        return self._remote_nodes[node_id]

    def _extract_port(self, url: str) -> int:
        from urllib.parse import urlparse
        parsed = urlparse(url)
        return parsed.port or 80

    def stop_node(self, node_id: str) -> dict:
        if node_id in self._remote_nodes:
            del self._remote_nodes[node_id]
            return {'node_id': node_id, 'status': 'removed'}
        if node_id not in self._processes:
            raise ValueError(f"Node '{node_id}' not found")

        info = self._processes[node_id]
        try:
            info.process.terminate()
            info.process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            info.process.kill()
            info.process.wait()

        del self._processes[node_id]
        return {'node_id': node_id, 'status': 'stopped'}

    def list_nodes(self) -> list[dict]:
        result = []
        for nid, info in self._processes.items():
            alive = info.process.poll() is None
            result.append({
                'node_id': nid,
                'node_type': info.node_type,
                'http_port': info.http_port,
                'tcp_port': info.tcp_port,
                'pid': info.pid,
                'alive': alive,
                'log_file': info.log_file,
                'is_remote': False,
            })
        for nid, info in self._remote_nodes.items():
            result.append({
                'node_id': nid,
                'node_type': info['node_type'],
                'http_port': info['http_port'],
                'tcp_port': info.get('tcp_port'),
                'pid': None,
                'alive': True,
                'log_file': None,
                'is_remote': True,
            })
        return result

    def get_node(self, node_id: str) -> Optional[dict]:
        if node_id in self._remote_nodes:
            info = self._remote_nodes[node_id]
            return {
                'node_id': info['node_id'],
                'node_type': info['node_type'],
                'http_port': info['http_port'],
                'tcp_port': info.get('tcp_port'),
                'pid': None,
                'alive': True,
                'log_file': None,
                'is_remote': True,
            }
        if node_id not in self._processes:
            return None
        info = self._processes[node_id]
        alive = info.process.poll() is None
        return {
            'node_id': info.node_id,
            'node_type': info.node_type,
            'http_port': info.http_port,
            'tcp_port': info.tcp_port,
            'pid': info.pid,
            'alive': alive,
            'log_file': info.log_file,
            'is_remote': False,
        }

    def get_master_url(self, node_id: str) -> Optional[str]:
        if node_id in self._remote_nodes:
            info = self._remote_nodes[node_id]
            return info['base_url'] if info['node_type'] == 'master' else None
        if node_id not in self._processes:
            return None
        info = self._processes[node_id]
        if info.node_type != 'master':
            return None
        return f'http://127.0.0.1:{info.port}'

    def get_node_url(self, node_id: str) -> Optional[str]:
        if node_id in self._remote_nodes:
            return self._remote_nodes[node_id]['base_url']
        if node_id not in self._processes:
            return None
        info = self._processes[node_id]
        return f'http://127.0.0.1:{info.port}'


manager = ProcessManager()
