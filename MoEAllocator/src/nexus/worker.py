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

logging.basicConfig(level=logging.INFO,
                    format='%(asctime)s [%(levelname)s] %(name)s: %(message)s')
logger = logging.getLogger("worker")


class ExpertWorker:
    def __init__(self, worker_id: str, http_port: int, tcp_port: int, bind_host: str = "127.0.0.1",
                 experts_dir: str = ""):
        self.worker_id = worker_id
        self.http_port = http_port
        self.tcp_port = tcp_port
        self.bind_host = bind_host
        self.experts_dir = experts_dir
        self.master_url: Optional[str] = None

        self.expert_weights: dict[tuple[int, int], dict[str, torch.Tensor]] = {}
        self.registered_experts: set[tuple[int, int]] = set()
        self._running = False

    async def _notify_master(self):
        if not self.master_url:
            return
        body = {
            "worker_id": self.worker_id,
            "host": self.bind_host,
            "http_port": self.http_port,
            "tcp_port": self.tcp_port,
            "experts_dir": self.experts_dir,
            "experts": [[lid, eid] for (lid, eid) in self.registered_experts],
        }
        try:
            async with aiohttp.ClientSession(trust_env=False) as sess:
                async with sess.post(f"{self.master_url}/workers", json=body, timeout=aiohttp.ClientTimeout(total=10)) as resp:
                    logger.info(f"Re-registered to master: {resp.status}")
        except Exception as e:
            logger.warning(f"Failed to re-register to master: {e}")

    async def _http_handler_status(self, request: web.Request) -> web.Response:
        loaded = sorted([f"L{lid:02d}_E{eid:03d}" for (lid, eid) in self.expert_weights.keys()])
        memory_mb = sum(
            sum(t.numel() * 4 for t in w.values()) / (1 << 20)
            for w in self.expert_weights.values()
        )
        return web.json_response({
            "worker_id": self.worker_id,
            "http_port": self.http_port,
            "tcp_port": self.tcp_port,
            "loaded_experts": loaded,
            "loaded_count": len(self.expert_weights),
            "memory_mb": round(memory_mb, 2),
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

            weights = storch.load_file(file_path)
            self.expert_weights[(layer_id, expert_id)] = {
                k: v.to(torch.float32) for k, v in weights.items()
            }
            self.registered_experts.add((layer_id, expert_id))

            size_mb = sum(t.numel() * 4 for t in weights.values()) / (1 << 20)
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
                weights = storch.load_file(file_path)
                self.expert_weights[(layer_id, expert_id)] = {
                    k: v.to(torch.float32) for k, v in weights.items()
                }
                self.registered_experts.add((layer_id, expert_id))
                size_mb = sum(t.numel() * 4 for t in weights.values()) / (1 << 20)
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
        logger.info(f"TCP connection from {addr}")
        try:
            header = await self._read_exact(reader, 24)
            layer_id, expert_id, batch_size, seq_len, hidden_size, dtype_flag = struct.unpack("!IIIIII", header)

            data_len = await self._read_exact(reader, 4)
            n_bytes = struct.unpack("!I", data_len)[0]
            hidden_data = await self._read_exact(reader, n_bytes)

            hidden = torch.frombuffer(hidden_data, dtype=torch.float32).reshape(batch_size, seq_len, hidden_size)

            key = (layer_id, expert_id)
            if key not in self.expert_weights:
                logger.error(f"Expert {expert_id}_layer_{layer_id} not loaded on {self.worker_id}")
                writer.write(struct.pack("!I", 0))
                await writer.drain()
                return

            weights = self.expert_weights[key]
            down_w = up_w = gate_w = None
            for k, v in weights.items():
                if k.endswith("down_proj.weight"): down_w = v
                elif k.endswith("up_proj.weight"): up_w = v
                elif k.endswith("gate_proj.weight"): gate_w = v

            if None in (down_w, up_w, gate_w):
                logger.error(f"Incomplete weights for expert_{expert_id}_layer_{layer_id}")
                writer.write(struct.pack("!I", 0))
                await writer.drain()
                return

            down = F.linear(
                torch.nn.functional.silu(F.linear(hidden, gate_w)) * F.linear(hidden, up_w),
                down_w
            )

            result_bytes = down.numpy().tobytes()
            writer.write(struct.pack("!I", len(result_bytes)))
            writer.write(result_bytes)
            await writer.drain()
            logger.debug(f"Processed expert_{expert_id}_layer_{layer_id}, sent {len(result_bytes)} bytes")

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
        self.master_url = master_url

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

        self._running = True
        logger.info(f"Worker {self.worker_id} ready. HTTP={self.bind_host}:{self.http_port}, TCP={self.bind_host}:{self.tcp_port}")

        async with tcp_server:
            await tcp_server.serve_forever()


def main():
    import argparse
    parser = argparse.ArgumentParser(description="MoEAllocator Expert Worker")
    parser.add_argument("--id", type=str, default=None, help="Worker ID")
    parser.add_argument("--http-port", type=int, default=8000, help="HTTP port")
    parser.add_argument("--tcp-port", type=int, default=9000, help="TCP port")
    parser.add_argument("--host", type=str, default="127.0.0.1", help="Bind host (default: 127.0.0.1)")
    parser.add_argument("--master", type=str, default="", help="Master URL (for auto-register)")
    parser.add_argument("--experts-dir", type=str, default="", help="Directory containing expert .safetensors files")
    parser.add_argument("--expert-ids", type=str, default="", help="Comma-separated expert IDs to auto-load (e.g., '3,4,5,6')")
    args = parser.parse_args()

    worker_id = args.id or f"worker-{args.http_port}"
    worker = ExpertWorker(worker_id, args.http_port, args.tcp_port, bind_host=args.host, experts_dir=args.experts_dir)

    if args.experts_dir and args.expert_ids:
        expert_ids = [int(x) for x in args.expert_ids.split(",")]
        logger.info(f"Auto-loading experts {expert_ids} from {args.experts_dir}")
        for eid in expert_ids:
            for lid in range(1, 28):
                file_name = f"expert_{eid:03d}_layer_{lid:03d}.safetensors"
                file_path = os.path.join(args.experts_dir, file_name)
                if os.path.exists(file_path):
                    weights = storch.load_file(file_path)
                    worker.expert_weights[(lid, eid)] = {k: v.to(torch.float32) for k, v in weights.items()}
                    worker.registered_experts.add((lid, eid))
        logger.info(f"Pre-loaded {len(worker.expert_weights)} expert files")

        if args.master:
            import urllib.request
            urllib.request.getproxies = lambda: {}
            experts = [[lid, eid] for (lid, eid) in worker.registered_experts]
            body = json.dumps({
                "worker_id": args.id,
                "host": args.host,
                "http_port": args.http_port,
                "tcp_port": args.tcp_port,
                "experts_dir": args.experts_dir,
                "experts": experts,
            }).encode()
            req = urllib.request.Request(args.master, data=body, method="POST", headers={"Content-Type": "application/json"})
            try:
                with urllib.request.urlopen(req, timeout=10) as r:
                    logger.info(f"Auto-registered: {json.loads(r.read())}")
            except Exception as e:
                logger.warning(f"Auto-register failed: {e}")

    try:
        asyncio.run(worker.start(master_url=args.master))
    except KeyboardInterrupt:
        logger.info(f"Worker {worker_id} shutting down")


if __name__ == "__main__":
    main()
