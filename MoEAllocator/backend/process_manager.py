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


class ProcessManager:
    _instance = None
    _lock = threading.Lock()

    def __new__(cls):
        if cls._instance is None:
            with cls._lock:
                if cls._instance is None:
                    cls._instance = super().__new__(cls)
                    cls._instance._processes = {}
                    cls._instance._base_dir = Path(__file__).parent.parent
                    cls._instance._clean_env = cls._instance._build_clean_env()
        return cls._instance

    def _build_clean_env(self):
        venv_python = str(self._base_dir / '.venv' / 'bin' / 'python')
        env = {k: v for k, v in os.environ.items() if 'proxy' not in k.lower()}
        venv_bin = str(self._base_dir / '.venv' / 'bin')
        env['PATH'] = venv_bin + ':' + os.environ.get('PATH', '')
        env['_PYTHON'] = venv_python
        return env

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

    def start_master(self, node_id: str, manifest_path: str, http_port: Optional[int] = None, expert_ids: Optional[str] = None) -> dict:
        if node_id in self._processes:
            raise ValueError(f"Node '{node_id}' already exists")

        port = http_port or self._find_free_port(5000)
        log_path = self._get_log_path('master', node_id)

        cmd = [
            self._base_dir / '.venv' / 'bin' / 'python', '-m', 'src.nexus.master',
            '--manifest', manifest_path,
            '--port', str(port),
        ]
        if expert_ids:
            cmd += ['--experts', expert_ids]

        process = subprocess.Popen(
            cmd,
            cwd=str(self._base_dir),
            env=self._clean_env,
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
                     experts_dir: Optional[str] = None, expert_ids: Optional[str] = None,
                     master_url: Optional[str] = None) -> dict:
        if node_id in self._processes:
            raise ValueError(f"Node '{node_id}' already exists")

        port = http_port or self._find_free_port(8000)
        tcp = tcp_port or self._find_free_port(9000)
        log_path = self._get_log_path('worker', node_id)

        cmd = [
            str(self._base_dir / '.venv' / 'bin' / 'python'), '-m', 'src.nexus.worker',
            '--id', node_id,
            '--http-port', str(port),
            '--tcp-port', str(tcp),
        ]
        if experts_dir:
            cmd += ['--experts-dir', experts_dir]
        if expert_ids:
            cmd += ['--expert-ids', expert_ids]
        if master_url:
            cmd += ['--master', f'{master_url}/workers']

        process = subprocess.Popen(
            cmd,
            cwd=str(self._base_dir),
            env=self._clean_env,
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

    def stop_node(self, node_id: str) -> dict:
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
            })
        return result

    def get_node(self, node_id: str) -> Optional[dict]:
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
        }

    def get_master_url(self, node_id: str) -> Optional[str]:
        if node_id not in self._processes:
            return None
        info = self._processes[node_id]
        if info.node_type != 'master':
            return None
        return f'http://127.0.0.1:{info.port}'

    def get_node_url(self, node_id: str) -> Optional[str]:
        if node_id not in self._processes:
            return None
        info = self._processes[node_id]
        return f'http://127.0.0.1:{info.port}'


manager = ProcessManager()
