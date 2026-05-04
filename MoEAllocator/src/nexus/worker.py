import asyncio
import json
import logging
import os
import struct
import sys
from pathlib import Path
from typing import Optional

sys.path.insert(0, str(Path(__file__).parent.parent.parent))

import safetensors.torch as storch
import torch
import torch.nn.functional as F
from aiohttp import web
import aiohttp

from src.nexus.startup import (
    format_ids,
    log_kv,
    log_runtime_info,
    normalize_master_register_url,
    path_status,
)

logging.basicConfig(level=logging.INFO,
                    format='%(asctime)s [%(levelname)s] %(name)s: %(message)s')
logger = logging.getLogger("worker")


DTYPE_TO_CODE = {torch.float32: 0, torch.float16: 1, torch.bfloat16: 2}
CODE_TO_DTYPE = {0: torch.float32, 1: torch.float16, 2: torch.bfloat16}


def _tensor_to_wire_bytes(tensor: torch.Tensor, dtype: torch.dtype) -> bytes:
    tensor = tensor.detach().cpu().contiguous().to(dtype)
    if dtype == torch.bfloat16:
        return tensor.view(torch.uint16).numpy().tobytes()
    return tensor.numpy().tobytes()


class ExpertWorker:
    def __init__(self, worker_id: str, http_port: int, tcp_port: int, bind_host: str = "127.0.0.1",
                 advertise_host: str = "", experts_dir: str = "", dtype: torch.dtype = torch.float32):
        self.worker_id = worker_id
        self.http_port = http_port
        self.tcp_port = tcp_port
        self.bind_host = bind_host
        self.advertise_host = advertise_host or bind_host
        self.experts_dir = experts_dir
        self.dtype = dtype
        self.master_url: Optional[str] = None

        self.expert_weights: dict[tuple[int, int], dict[str, torch.Tensor]] = {}
        self.registered_experts: set[tuple[int, int]] = set()
        self._running = False

    def _tensor_nbytes(self, tensor: torch.Tensor) -> int:
        return tensor.numel() * tensor.element_size()

    def memory_mb(self) -> float:
        total = sum(
            sum(self._tensor_nbytes(t) for t in w.values())
            for w in self.expert_weights.values()
        )
        return total / (1 << 20)

    def _load_expert_file(self, expert_id: int, layer_id: int, file_path: str) -> float:
        weights = storch.load_file(file_path)
        self.expert_weights[(layer_id, expert_id)] = {
            k: v.to(self.dtype) for k, v in weights.items()
        }
        self.registered_experts.add((layer_id, expert_id))
        return sum(self._tensor_nbytes(t) for t in self.expert_weights[(layer_id, expert_id)].values()) / (1 << 20)

    async def _notify_master(self):
        if not self.master_url:
            return
        body = {
            "worker_id": self.worker_id,
            "host": self.advertise_host,
            "http_port": self.http_port,
            "tcp_port": self.tcp_port,
            "experts_dir": self.experts_dir,
            "experts": [[lid, eid] for (lid, eid) in self.registered_experts],
        }
        try:
            async with aiohttp.ClientSession(trust_env=False) as sess:
                async with sess.post(self.master_url, json=body, timeout=aiohttp.ClientTimeout(total=10)) as resp:
                    logger.info(f"Re-registered to master: {resp.status}")
        except Exception as e:
            logger.warning(f"Failed to re-register to master: {e}")

    async def _http_handler_status(self, request: web.Request) -> web.Response:
        loaded = sorted([f"L{lid:02d}_E{eid:03d}" for (lid, eid) in self.expert_weights.keys()])
        return web.json_response({
            "worker_id": self.worker_id,
            "http_port": self.http_port,
            "tcp_port": self.tcp_port,
            "runtime_dtype": str(self.dtype).replace("torch.", ""),
            "loaded_experts": loaded,
            "loaded_count": len(self.expert_weights),
            "memory_mb": round(self.memory_mb(), 2),
        })

    def _expert_file_path(self, expert_id: int, layer_id: int) -> str:
        return os.path.join(self.experts_dir, f"expert_{expert_id:03d}_layer_{layer_id:03d}.safetensors")

    async def _http_handler_load(self, request: web.Request) -> web.Response:
        try:
            body = await request.json()
            expert_id = int(body["expert_id"])
            layer_id = int(body["layer_id"])
            file_path = body.get("file_path") or self._expert_file_path(expert_id, layer_id)

            if not os.path.exists(file_path):
                return web.json_response({"error": f"File not found: {file_path}"}, status=404)

            size_mb = self._load_expert_file(expert_id, layer_id, file_path)
            logger.info(f"Loaded expert_{expert_id}_layer_{layer_id} ({size_mb:.1f} MB)")
            return web.json_response({"status": "ok", "expert_id": expert_id, "layer_id": layer_id})
        except Exception as e:
            logger.error(f"Load failed: {e}")
            return web.json_response({"error": str(e)}, status=500)

    async def _http_handler_batch_load(self, request: web.Request) -> web.Response:
        try:
            body = await request.json()
            tasks = body.get("tasks", [])
            loaded = []
            failed = []
            for task in tasks:
                expert_id = int(task["expert_id"])
                layer_id = int(task["layer_id"])
                file_path = task.get("file_path") or self._expert_file_path(expert_id, layer_id)
                if not os.path.exists(file_path):
                    failed.append({"expert_id": expert_id, "layer_id": layer_id, "error": f"File not found: {file_path}"})
                    continue
                size_mb = self._load_expert_file(expert_id, layer_id, file_path)
                loaded.append({"expert_id": expert_id, "layer_id": layer_id, "size_mb": round(size_mb, 1)})
                logger.info(f"Loaded expert_{expert_id}_layer_{layer_id} ({size_mb:.1f} MB)")
            resp = web.json_response({"loaded": loaded, "failed": failed, "total": len(self.expert_weights)})
            asyncio.create_task(self._notify_master())
            return resp
        except Exception as e:
            return web.json_response({"error": str(e)}, status=500)

    async def _http_handler_unload(self, request: web.Request) -> web.Response:
        try:
            body = await request.json()
            expert_id = int(body["expert_id"])
            layer_id = int(body["layer_id"])
            key = (layer_id, expert_id)
            if key in self.expert_weights:
                del self.expert_weights[key]
                self.registered_experts.discard(key)
                logger.info(f"Unloaded expert_{expert_id}_layer_{layer_id}")
                asyncio.create_task(self._notify_master())
                return web.json_response({"status": "ok"})
            return web.json_response({"error": "Expert not loaded"}, status=404)
        except Exception as e:
            return web.json_response({"error": str(e)}, status=500)

    async def _http_handler_register(self, request: web.Request) -> web.Response:
        try:
            body = await request.json()
            new_master_url = body.get("master_url", "")
            if new_master_url:
                self.master_url = new_master_url
            experts = body.get("experts", [])
            if experts:
                self.registered_experts.update((lid, eid) for (lid, eid) in experts)
            logger.info(f"Registered with master, {len(experts)} experts available")
            return web.json_response({
                "status": "ok",
                "worker_id": self.worker_id,
                "http_port": self.http_port,
                "tcp_port": self.tcp_port,
            })
        except Exception as e:
            return web.json_response({"error": str(e)}, status=500)

    async def _read_exact(self, reader: asyncio.StreamReader, n: int) -> bytes:
        data = b""
        while len(data) < n:
            chunk = await reader.read(n - len(data))
            if not chunk:
                break
            data += chunk
        return data

    async def _tcp_handler(self, reader: asyncio.StreamReader, writer: asyncio.StreamWriter):
        addr = writer.get_extra_info('peername')
        try:
            header = await self._read_exact(reader, 24)
            if len(header) != 24:
                logger.warning(f"TCP short header from {addr}: expected 24 bytes, got {len(header)}")
                return
            layer_id, expert_id, batch_size, seq_len, hidden_size, dtype_code = struct.unpack("!IIIIII", header)
            dtype = CODE_TO_DTYPE.get(dtype_code, torch.float32)
            logger.info(f"TCP recv: expert_{expert_id}_L{layer_id} shape=({batch_size},{seq_len},{hidden_size}) dtype={dtype}")

            data_len = await self._read_exact(reader, 4)
            if len(data_len) != 4:
                logger.warning(f"TCP short data length from {addr}: expected 4 bytes, got {len(data_len)}")
                return
            n_bytes = struct.unpack("!I", data_len)[0]
            hidden_data = await self._read_exact(reader, n_bytes)
            if len(hidden_data) != n_bytes:
                logger.warning(f"TCP short payload from {addr}: expected {n_bytes} bytes, got {len(hidden_data)}")
                writer.write(struct.pack("!I", 0))
                await writer.drain()
                return
            hidden = torch.frombuffer(hidden_data, dtype=dtype).reshape(batch_size, seq_len, hidden_size).clone()

            key = (layer_id, expert_id)
            if key not in self.expert_weights:
                logger.error(f"Expert {expert_id}_layer_{layer_id} not loaded on {self.worker_id}")
                writer.write(struct.pack("!I", 0))
                await writer.drain()
                return

            weights = self.expert_weights[key]
            down_w = up_w = gate_w = None
            for k, v in weights.items():
                if k.endswith("down_proj.weight"): down_w = v.to(dtype)
                elif k.endswith("up_proj.weight"): up_w = v.to(dtype)
                elif k.endswith("gate_proj.weight"): gate_w = v.to(dtype)

            if None in (down_w, up_w, gate_w):
                logger.error(f"Incomplete weights for expert_{expert_id}_layer_{layer_id}")
                writer.write(struct.pack("!I", 0))
                await writer.drain()
                return

            down = F.linear(
                torch.nn.functional.silu(F.linear(hidden, gate_w)) * F.linear(hidden, up_w),
                down_w
            )

            result_bytes = _tensor_to_wire_bytes(down, dtype)
            writer.write(struct.pack("!I", len(result_bytes)))
            writer.write(result_bytes)
            await writer.drain()
            logger.info(f"TCP send: expert_{expert_id}_L{layer_id} result={down.shape}")

        except Exception as e:
            logger.error(f"TCP handler error: {e}")
        finally:
            writer.close()
            await writer.wait_closed()

    async def _http_server(self, app: web.Application):
        runner = web.AppRunner(app)
        await runner.setup()
        site = web.TCPSite(runner, self.bind_host, self.http_port)
        await site.start()
        logger.info(f"HTTP server started on {self.bind_host}:{self.http_port}")

    async def start(self, master_url: str = ""):
        self.master_url = normalize_master_register_url(master_url)

        app = web.Application()
        app.router.add_get("/status", self._http_handler_status)
        app.router.add_post("/load", self._http_handler_load)
        app.router.add_post("/unload", self._http_handler_unload)
        app.router.add_post("/batch_load", self._http_handler_batch_load)
        app.router.add_post("/register", self._http_handler_register)

        await self._http_server(app)

        tcp_server = await asyncio.start_server(
            self._tcp_handler, self.bind_host, self.tcp_port
        )
        logger.info(f"TCP server started on port {self.tcp_port}")

        if self.master_url:
            await self._notify_master()

        self._running = True
        logger.info(
            f"Worker {self.worker_id} ready. HTTP={self.bind_host}:{self.http_port}, TCP={self.bind_host}:{self.tcp_port}, "
            f"loaded_experts={len(self.expert_weights)}, memory={self.memory_mb():.1f} MB"
        )

        async with tcp_server:
            await tcp_server.serve_forever()


def main():
    import argparse
    parser = argparse.ArgumentParser(description="MoEAllocator Expert Worker")
    parser.add_argument("--id", type=str, default=None, help="Worker ID")
    parser.add_argument("--http-port", type=int, default=8000, help="HTTP port")
    parser.add_argument("--tcp-port", type=int, default=9000, help="TCP port")
    parser.add_argument("--host", type=str, default="127.0.0.1", help="Bind host (default: 127.0.0.1)")
    parser.add_argument("--advertise-host", type=str, default="",
        help="Address Master uses to reach this worker (default: same as --host)")
    parser.add_argument("--master", type=str, default="", help="Master URL (for auto-register)")
    parser.add_argument("--experts-dir", type=str, default="", help="Directory containing expert .safetensors files")
    parser.add_argument("--expert-ids", type=str, default="", help="Comma-separated expert IDs to auto-load (e.g., '3,4,5,6')")
    parser.add_argument("--dtype", "-d", type=str, default="float32",
        choices=["float32", "fp16", "float16", "bf16", "bfloat16"],
        help="Weight precision: float32 (default), fp16/float16, bf16/bfloat16")
    parser.add_argument("--log-file", type=str, default="",
        help="Log file path (default: stdout only)")
    args = parser.parse_args()

    if args.log_file:
        Path(args.log_file).parent.mkdir(parents=True, exist_ok=True)
        fh = logging.FileHandler(args.log_file)
        fh.setFormatter(logging.Formatter('%(asctime)s [%(levelname)s] %(name)s: %(message)s'))
        logging.getLogger().addHandler(fh)

    dtype_map = {"float32": torch.float32, "fp16": torch.float16, "float16": torch.float16,
                  "bf16": torch.bfloat16, "bfloat16": torch.bfloat16}
    dtype = dtype_map[args.dtype]
    worker_id = args.id or f"worker-{args.http_port}"
    master_register_url = normalize_master_register_url(args.master)
    log_runtime_info(logger, "worker")
    log_kv(
        logger,
        "worker_config",
        [
            ("id", worker_id),
            ("bind_host", args.host),
            ("advertise_host", args.advertise_host or args.host),
            ("http_port", args.http_port),
            ("tcp_port", args.tcp_port),
            ("runtime_dtype_arg", args.dtype),
            ("runtime_dtype_effective", str(dtype).replace("torch.", "")),
            ("experts_dir", path_status(args.experts_dir)),
            ("expert_ids", format_ids(args.expert_ids)),
            ("master_register_url", master_register_url),
        ],
    )
    worker = ExpertWorker(worker_id, args.http_port, args.tcp_port, bind_host=args.host,
                         advertise_host=args.advertise_host, experts_dir=args.experts_dir, dtype=dtype)

    if args.experts_dir and args.expert_ids:
        expert_ids = [int(x) for x in args.expert_ids.split(",")]
        logger.info(f"Auto-loading experts {expert_ids} from {args.experts_dir}")
        missing = 0
        for eid in expert_ids:
            for lid in range(1, 28):
                file_name = f"expert_{eid:03d}_layer_{lid:03d}.safetensors"
                file_path = os.path.join(args.experts_dir, file_name)
                if os.path.exists(file_path):
                    worker._load_expert_file(eid, lid, file_path)
                else:
                    missing += 1
        logger.info(
            f"Pre-loaded {len(worker.expert_weights)} expert files, missing={missing}, memory={worker.memory_mb():.1f} MB"
        )
        if len(worker.expert_weights) == 0:
            logger.warning("No expert files were loaded. Check --experts-dir, --expert-ids, and layer range.")

        if master_register_url:
            import urllib.request
            urllib.request.getproxies = lambda: {}
            experts = [[lid, eid] for (lid, eid) in worker.registered_experts]
            body = json.dumps({
                "worker_id": worker_id,
                "host": worker.advertise_host,
                "http_port": args.http_port,
                "tcp_port": args.tcp_port,
                "experts_dir": args.experts_dir,
                "experts": experts,
            }).encode()
            req = urllib.request.Request(master_register_url, data=body, method="POST", headers={"Content-Type": "application/json"})
            try:
                with urllib.request.urlopen(req, timeout=10) as r:
                    logger.info(f"Auto-registered: {json.loads(r.read())}")
            except Exception as e:
                logger.warning(f"Auto-register failed: {e}")

    try:
        asyncio.run(worker.start(master_url=master_register_url))
    except KeyboardInterrupt:
        logger.info(f"Worker {worker_id} shutting down")


if __name__ == "__main__":
    main()
