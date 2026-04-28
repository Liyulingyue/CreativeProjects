import asyncio
import json
import logging
import math
import os
import struct
import sys
from dataclasses import dataclass, field
from pathlib import Path
from typing import Optional

sys.path.insert(0, str(Path(__file__).parent.parent.parent))

import aiohttp
import safetensors.torch as storch
import torch
import torch.nn.functional as F
import aiohttp
from aiohttp import web

logging.basicConfig(level=logging.INFO,
                    format='%(asctime)s [%(levelname)s] %(name)s: %(message)s')
logger = logging.getLogger("nexus")


def rotate_half(x):
    x1, x2 = x[..., :x.shape[-1] // 2], x[..., x.shape[-1] // 2:]
    return torch.cat([-x2, x1], dim=-1)


@dataclass
class WorkerInfo:
    worker_id: str
    host: str
    http_port: int
    tcp_port: int
    experts_dir: str = ""
    loaded_experts: set[tuple[int, int]] = field(default_factory=set)


class NexusMaster:
    def __init__(self, manifest_path: str, http_port: int = 5000, bind_host: str = "127.0.0.1",
                 local_expert_ids: list[int] | None = None, dtype: torch.dtype = torch.float32,
                 kv_cache: bool = False):
        self.manifest_path = manifest_path
        self.http_port = http_port
        self.bind_host = bind_host
        self.local_expert_ids = local_expert_ids or []
        self.dtype = dtype
        self.kv_cache = kv_cache
        self._past_keys: dict[int, torch.Tensor] = {}
        self._past_values: dict[int, torch.Tensor] = {}

        with open(manifest_path) as f:
            data = json.load(f)
        self.cfg = data["config"]
        self.output_dir = Path(data["output_dir"])

        self.hidden_size = self.cfg["hidden_size"]
        self.num_attention_heads = self.cfg.get("num_attention_heads", 20)
        self.num_kv_heads = self.cfg.get("num_key_value_heads", 4)
        self.head_dim = self.hidden_size // self.num_attention_heads
        self.num_hidden_layers = self.cfg["num_layers"]
        self.moe_k = self.cfg["moe_k"]
        self.num_shared_experts = self.cfg["num_shared_experts"]
        self.moe_intermediate = self.cfg["moe_intermediate_size"]
        self.rope_theta = self.cfg.get("rope_theta", 500000.0)
        self.rms_norm_eps = self.cfg.get("rms_norm_eps", 1e-5)
        self.intermediate_size = self.cfg.get("intermediate_size", 12288)

        self.weights: dict[str, torch.Tensor] = {}
        self.local_experts: dict[tuple[int, int], dict[str, torch.Tensor]] = {}
        self.workers: dict[str, WorkerInfo] = {}
        self._local_routing: set[tuple[int, int]] = set()
        self._session: Optional[aiohttp.ClientSession] = None

    def load_backbone(self):
        logger.info("Loading backbone...")
        backbone_path = self.output_dir / "backbone" / "backbone.safetensors"
        w = storch.load_file(str(backbone_path))
        for k, v in w.items():
            self.weights[k] = v.to(self.dtype)
        logger.info(f"  {len(w)} backbone tensors")

        gate_path = self.output_dir / "backbone" / "gate.safetensors"
        if gate_path.exists():
            gw = storch.load_file(str(gate_path))
            for k, v in gw.items():
                self.weights[k] = v.to(self.dtype)
            logger.info(f"  {len(gw)} gate tensors")

        shared_path = self.output_dir / "backbone" / "shared_expert.safetensors"
        if shared_path.exists():
            shared = storch.load_file(str(shared_path))
            for k, v in shared.items():
                self.weights[k] = v.to(self.dtype)
            logger.info(f"  {len(shared)} shared expert tensors")

    def load_local_experts(self, expert_ids: list[int]):
        logger.info(f"Loading local experts {expert_ids}...")
        for eid in expert_ids:
            for lid in range(1, self.num_hidden_layers + 1):
                fp = self.output_dir / "experts" / f"expert_{eid:03d}_layer_{lid:03d}.safetensors"
                if fp.exists():
                    w = storch.load_file(str(fp))
                    self.local_experts[(lid, eid)] = {k: v.to(self.dtype) for k, v in w.items()}
                    self._local_routing.add((lid, eid))
        logger.info(f"  Loaded {len(self.local_experts)} local expert files, _local_routing size={len(self._local_routing)}")

    def load(self, expert_ids: list[int] | None = None):
        self.load_backbone()
        if expert_ids is None:
            expert_ids = self.local_expert_ids
        if expert_ids:
            self.load_local_experts(expert_ids)

    def load_expert(self, expert_id: int, layer_id: int):
        fp = self.output_dir / "experts" / f"expert_{expert_id:03d}_layer_{layer_id:03d}.safetensors"
        if not fp.exists():
            raise FileNotFoundError(f"Expert file not found: {fp}")
        w = storch.load_file(str(fp))
        self.local_experts[(layer_id, expert_id)] = {k: v.to(self.dtype) for k, v in w.items()}
        self._local_routing.add((layer_id, expert_id))
        size_mb = sum(t.numel() * 4 for t in w.values()) / (1 << 20)
        return size_mb

    def unload_expert(self, expert_id: int, layer_id: int):
        key = (layer_id, expert_id)
        if key not in self.local_experts:
            raise KeyError(f"Expert (layer={layer_id}, id={expert_id}) not loaded")
        del self.local_experts[key]
        self._local_routing.discard(key)
        return True

    def _rms_norm(self, x, weight):
        input_dtype = x.dtype
        x = x.to(torch.float32)
        variance = x.pow(2).mean(-1, keepdim=True)
        x = x * torch.rsqrt(variance + self.rms_norm_eps)
        return (weight * x).to(input_dtype)

    def _rope_compute(self, seq_len, device):
        inv_freq = 1.0 / (self.rope_theta ** (torch.arange(0, self.head_dim, 2, dtype=torch.float32, device=device) / self.head_dim))
        t = torch.arange(seq_len, dtype=torch.float32, device=device)
        freqs = torch.outer(t, inv_freq)
        freqs = torch.cat([freqs, freqs], dim=-1)
        return freqs.cos(), freqs.sin()

    def _rotate_half(self, x):
        x1 = x[..., 0::2]
        x2 = x[..., 1::2]
        return torch.stack((-x2, x1), dim=-1).flatten(-2)

    def _attention(self, x, layer_id: int):
        prefix = f"model.layers.{layer_id}.self_attn"
        q = F.linear(x, self.weights[f"{prefix}.q_proj.weight"])
        k = F.linear(x, self.weights[f"{prefix}.k_proj.weight"])
        v = F.linear(x, self.weights[f"{prefix}.v_proj.weight"])

        bsz, seq_len, _ = x.shape
        import sys; print(f"[DEBUG _attention layer {layer_id}] seq_len={seq_len}", flush=True)
        cos, sin = self._rope_compute(seq_len, x.device)
        import sys; print(f"[DEBUG _attention layer {layer_id}] cos.shape={cos.shape}, sin.shape={sin.shape}, seq_len_arg={seq_len}", flush=True)
        print(f"[DEBUG _attention layer {layer_id}] q.shape={q.shape}, k.shape={k.shape}", flush=True)
        print(f"[DEBUG _attention layer {layer_id}] q_proj={self.weights[prefix+'.q_proj.weight'].shape}, k_proj={self.weights[prefix+'.k_proj.weight'].shape}", flush=True)
        q = q.view(bsz, seq_len, self.num_attention_heads, self.head_dim).transpose(1, 2)
        k = k.view(bsz, seq_len, self.num_kv_heads, self.head_dim).transpose(1, 2)
        v = v.view(bsz, seq_len, self.num_kv_heads, self.head_dim).transpose(1, 2)
        import sys; print(f"[DEBUG _attention layer {layer_id}] AFTER view+transpose: q.shape={q.shape}, k.shape={k.shape}", flush=True)
        cos = cos.unsqueeze(0).unsqueeze(0)
        sin = sin.unsqueeze(0).unsqueeze(0)
        q = (q.float() * cos + self._rotate_half(q).float() * sin).to(q.dtype)
        k = (k.float() * cos + self._rotate_half(k).float() * sin).to(k.dtype)

        if self.num_attention_heads != self.num_kv_heads:
            n_rep = self.num_attention_heads // self.num_kv_heads
            k = k.transpose(1, 2).repeat_interleave(n_rep, dim=1)
            v = v.transpose(1, 2).repeat_interleave(n_rep, dim=1)
        else:
            k = k.transpose(1, 2)
            v = v.transpose(1, 2)

        scaling = self.head_dim ** -0.5
        attn = torch.matmul(q, k.transpose(-2, -1)) * scaling
        attn = F.softmax(attn, dim=-1)
        out = torch.matmul(attn, v).transpose(1, 2).contiguous().view(bsz, seq_len, self.hidden_size)
        return F.linear(out, self.weights[f"{prefix}.o_proj.weight"])

    def _ffn(self, x, down, up, gate):
        return F.linear(
            torch.nn.functional.silu(F.linear(x, gate)) * F.linear(x, up),
            down
        )

    def _gate_forward(self, x, layer_id: int):
        gate_weight = self.weights[f"model.layers.{layer_id}.mlp.gate.weight"]
        return F.linear(x.float(), gate_weight.float())

    def _moe_expert_local(self, x, layer_id: int, expert_id: int):
        key = (layer_id, expert_id)
        if key not in self.local_experts:
            return None
        w = self.local_experts[key]
        down = up = gate = None
        for k, v in w.items():
            if k.endswith("down_proj.weight"): down = v
            elif k.endswith("up_proj.weight"): up = v
            elif k.endswith("gate_proj.weight"): gate = v
        if None in (down, up, gate):
            return None
        return self._ffn(x, down, up, gate)

    async def _dispatch_expert(self, worker: WorkerInfo, layer_id: int, expert_id: int, hidden: torch.Tensor, rank: int = 0):
        try:
            logger.info(f"  -> dispatch token-r{rank} expert_{expert_id}_layer_{layer_id} to {worker.worker_id}")
            reader, writer = await asyncio.wait_for(
                asyncio.open_connection(worker.host, worker.tcp_port),
                timeout=30.0
            )
            dtype_code = {torch.float32: 0, torch.float16: 1, torch.bfloat16: 2}.get(self.dtype, 0)
            header = struct.pack("!IIIIII", layer_id, expert_id, hidden.shape[0], hidden.shape[1], hidden.shape[2], dtype_code)
            data = hidden.to(self.dtype).numpy().tobytes()
            writer.write(header)
            writer.write(struct.pack("!I", len(data)))
            writer.write(data)
            await writer.drain()

            size_data = await reader.readexactly(4)
            size = struct.unpack("!I", size_data)[0]
            result_data = b""
            while len(result_data) < size:
                chunk = await reader.read(size - len(result_data))
                if not chunk:
                    break
                result_data += chunk
            writer.close()
            await writer.wait_closed()

            if size == 0:
                return None
            result = torch.frombuffer(result_data, dtype=torch.float32).reshape(hidden.shape)
            logger.info(f"  <- received token-r{rank} expert_{expert_id}_layer_{layer_id} from {worker.worker_id}")
            return result
        except Exception as e:
            logger.error(f"TCP dispatch to worker {worker.worker_id} failed: {e}")
            return None

    def _shared_expert(self, x, layer_id: int):
        prefix = f"model.layers.{layer_id}.mlp.shared_experts"
        down = self.weights.get(f"{prefix}.down_proj.weight")
        up = self.weights.get(f"{prefix}.up_proj.weight")
        gate = self.weights.get(f"{prefix}.gate_proj.weight")
        if None in (down, up, gate):
            return torch.zeros_like(x)
        return self._ffn(x, down, up, gate)

    def _standard_mlp(self, x, layer_id: int):
        prefix = f"model.layers.{layer_id}.mlp"
        down = self.weights[f"{prefix}.down_proj.weight"]
        up = self.weights[f"{prefix}.up_proj.weight"]
        gate = self.weights[f"{prefix}.gate_proj.weight"]
        return self._ffn(x, down, up, gate)

    def _build_routing_table(self) -> dict[tuple[int, int], WorkerInfo]:
        routing: dict[tuple[int, int], WorkerInfo] = {}
        for wid, worker in self.workers.items():
            for exp in worker.loaded_experts:
                routing[exp] = worker
        return routing

    async def _moe_layer(self, x_norm: torch.Tensor, layer_id: int) -> torch.Tensor:
        moe_out = torch.zeros_like(x_norm)
        for _ in range(self.num_shared_experts):
            moe_out = moe_out + self._shared_expert(x_norm, layer_id)

        gate_logits = self._gate_forward(x_norm, layer_id)
        bsz, seq_len, num_experts = gate_logits.shape
        logger.info(f"  Layer {layer_id}: _local_routing={len(self._local_routing)}, workers={list(self.workers.keys())}")

        gate_flat = gate_logits.view(-1, num_experts)
        routing_probs = F.softmax(gate_flat, dim=-1)
        topk_probs, topk_indices = torch.topk(routing_probs, k=min(self.moe_k, num_experts), dim=-1)
        num_tokens = gate_flat.shape[0]
        K = topk_indices.shape[1]

        routing = self._build_routing_table()

        shown = min(2, num_tokens)
        orig_list = [topk_indices[t][:K].tolist() for t in range(shown)]
        logger.info(f"  Layer {layer_id}: first {shown} token(s) gate picks: {orig_list}")

        tasks = {}
        local_exec = 0
        dispatched = 0
        fallback_total = 0

        for tok_idx in range(num_tokens):
            tok_probs = routing_probs[tok_idx]
            tok_topk_indices = topk_indices[tok_idx]
            tok_topk_probs = topk_probs[tok_idx]
            sorted_idx_by_prob = torch.argsort(tok_probs, descending=True)

            selected = []
            used_expert_ids = set()
            for eid in sorted_idx_by_prob.tolist():
                if len(selected) >= K:
                    break
                key = (layer_id, eid)
                if key in self._local_routing:
                    selected.append((eid, "local", tok_probs[eid].item()))
                    used_expert_ids.add(eid)
                elif key in routing:
                    selected.append((eid, "dispatch", tok_probs[eid].item()))
                    used_expert_ids.add(eid)

            if len(selected) < K:
                for eid in sorted_idx_by_prob.tolist():
                    if len(selected) >= K:
                        break
                    if eid in used_expert_ids:
                        continue
                    key = (layer_id, eid)
                    if key in self._local_routing:
                        selected.append((eid, "fallback_local", tok_probs[eid].item()))
                        used_expert_ids.add(eid)
                        fallback_total += 1
                    elif key in routing:
                        selected.append((eid, "fallback_dispatch", tok_probs[eid].item()))
                        used_expert_ids.add(eid)
                        fallback_total += 1

            if tok_idx == 0:
                logger.info(f"  Layer {layer_id}: token0 selected={[e for e, s, _ in selected]}")

            norm = tok_topk_probs.sum().clamp(min=1e-9)

            for rank, (expert_id, source, prob) in enumerate(selected):
                weight = prob / norm

                key = (layer_id, expert_id)
                if source in ("local", "fallback_local"):
                    e_out = self._moe_expert_local(x_norm, layer_id, expert_id)
                    if e_out is not None:
                        moe_out = moe_out + e_out * weight
                        local_exec += 1
                else:
                    worker = routing[key]
                    task = asyncio.create_task(
                        self._dispatch_expert(worker, layer_id, expert_id, x_norm, rank=rank)
                    )
                    tasks[(tok_idx, rank)] = (expert_id, task, weight)
                    dispatched += 1

        if tasks:
            task_list = [t for _, t, _ in tasks.values()]
            results = await asyncio.gather(*task_list, return_exceptions=True)
            for (tok_idx, rank), result in zip(tasks.keys(), results):
                if isinstance(result, torch.Tensor):
                    weight = tasks[(tok_idx, rank)][2]
                    moe_out = moe_out + result * weight
                else:
                    logger.error(f"Expert {tasks[(tok_idx, rank)][0]} failed: {result}")

        logger.info(f"  Layer {layer_id}: local={local_exec}, dispatched={dispatched}, fallback={fallback_total}, total={num_tokens * K}")

        return moe_out

    async def _transformer_layer(self, x, layer_id: int):
        import sys; print(f"[DEBUG layer {layer_id}] x.shape={x.shape}", flush=True)
        x_norm = self._rms_norm(x, self.weights[f"model.layers.{layer_id}.input_layernorm.weight"])
        x = x + self._attention(x_norm, layer_id)

        x_norm = self._rms_norm(x, self.weights[f"model.layers.{layer_id}.post_attention_layernorm.weight"])
        if layer_id == 0:
            x = x + self._standard_mlp(x_norm, layer_id)
        else:
            moe_out = await self._moe_layer(x_norm, layer_id)
            x = x + moe_out
        return x

    async def forward(self, input_ids: torch.Tensor) -> torch.Tensor:
        x = F.embedding(input_ids, self.weights["model.embed_tokens.weight"])
        for lid in range(self.num_hidden_layers):
            x = await self._transformer_layer(x, lid)
        x = self._rms_norm(x, self.weights["model.norm.weight"])
        return F.linear(x, self.weights["model.embed_tokens.weight"])

    @torch.no_grad()
    async def generate(self, input_ids: torch.Tensor, max_new_tokens: int = 50, eos_token_id: int = 2) -> list[int]:
        import sys; print(f"\nGenerating (max_new_tokens={max_new_tokens}), input_ids.shape={input_ids.shape}", flush=True)
        generated = input_ids[0].tolist()
        for step in range(max_new_tokens):
            logits = await self.forward(input_ids)
            next_token = logits[0, -1].argmax().item()
            generated.append(next_token)
            if next_token == eos_token_id:
                break
            new_ids = torch.tensor([[next_token]], dtype=input_ids.dtype)
            input_ids = torch.cat([input_ids, new_ids], dim=1)
            if (step + 1) % 10 == 0:
                print(f"  Step {step + 1}/{max_new_tokens}, seq_len={input_ids.shape[1]}")
        return generated

    async def _http_handler_worker_register(self, request: web.Request) -> web.Response:
        try:
            body = await request.json()
            worker_id = body.get("worker_id", f"worker-{len(self.workers)}")
            host = body.get("host") or request.remote or "127.0.0.1"
            http_port = body.get("http_port", 8000)
            tcp_port = body.get("tcp_port", 9000)
            experts_dir = body.get("experts_dir", "")
            experts = body.get("experts", [])
            loaded_experts = set()
            for item in experts:
                if isinstance(item, (list, tuple)) and len(item) >= 2:
                    loaded_experts.add((item[0], item[1]))
            self.workers[worker_id] = WorkerInfo(
                worker_id=worker_id,
                host=host,
                http_port=http_port,
                tcp_port=tcp_port,
                experts_dir=experts_dir,
                loaded_experts=loaded_experts,
            )
            logger.info(f"Worker registered: {worker_id} at {host}:{http_port}/{tcp_port}, experts_dir={experts_dir}, experts={len(loaded_experts)}")
            return web.json_response({"status": "ok", "worker_id": worker_id})
        except Exception as e:
            return web.json_response({"error": str(e)}, status=500)

    async def _http_handler_workers(self, request: web.Request) -> web.Response:
        return web.json_response({
            "workers": {
                wid: {
                    "host": w.host,
                    "http_port": w.http_port,
                    "tcp_port": w.tcp_port,
                    "loaded_experts": sorted([f"L{l:02d}_E{e:03d}" for (l, e) in w.loaded_experts]),
                    "note": "Use /workers/{id}/status for real-time query",
                }
                for wid, w in self.workers.items()
            }
        })

    async def _http_handler_worker_status(self, request: web.Request) -> web.Response:
        wid = request.match_info["worker_id"]
        if wid not in self.workers:
            return web.json_response({"error": "Worker not found"}, status=404)
        w = self.workers[wid]
        try:
            async with aiohttp.ClientSession() as session:
                async with session.get(f"http://{w.host}:{w.http_port}/status", timeout=5) as resp:
                    worker_status = await resp.json()
            return web.json_response(worker_status)
        except Exception as e:
            return web.json_response({
                "worker_id": w.worker_id,
                "host": w.host,
                "http_port": w.http_port,
                "tcp_port": w.tcp_port,
                "error": f"Failed to query worker: {e}",
            })

    def _expert_file_path(self, expert_id: int, layer_id: int) -> str:
        return str(self.output_dir / "experts" / f"expert_{expert_id:03d}_layer_{layer_id:03d}.safetensors")

    async def _http_handler_worker_load(self, request: web.Request) -> web.Response:
        wid = request.match_info["worker_id"]
        if wid not in self.workers:
            return web.json_response({"error": "Worker not found"}, status=404)
        body = await request.json()
        expert_id = body["expert_id"]
        layer_id = body["layer_id"]
        w = self.workers[wid]
        file_path = body.get("file_path") or self._expert_file_path(int(expert_id), int(layer_id))
        try:
            async with self._session.post(f"http://{w.host}:{w.http_port}/load", json={
                "expert_id": expert_id, "layer_id": layer_id, "file_path": file_path
            }) as resp:
                result = await resp.json()
            return web.json_response(result)
        except Exception as e:
            return web.json_response({"error": str(e)}, status=500)

    async def _http_handler_worker_unload(self, request: web.Request) -> web.Response:
        wid = request.match_info["worker_id"]
        if wid not in self.workers:
            return web.json_response({"error": "Worker not found"}, status=404)
        body = await request.json()
        w = self.workers[wid]
        try:
            async with self._session.post(f"http://{w.host}:{w.http_port}/unload", json=body) as resp:
                result = await resp.json()
            return web.json_response(result)
        except Exception as e:
            return web.json_response({"error": str(e)}, status=500)

    async def _http_handler_load_expert(self, request: web.Request) -> web.Response:
        try:
            body = await request.json()
            expert_id = body.get("expert_id")
            layer_id = body.get("layer_id")
            if expert_id is None or layer_id is None:
                return web.json_response({"error": f"expert_id and layer_id are required, got expert_id={expert_id!r}, layer_id={layer_id!r}"}, status=400)
            expert_id = int(expert_id)
            layer_id = int(layer_id)
            size_mb = self.load_expert(expert_id, layer_id)
            local_loaded = sorted([f"L{l:02d}_E{e:03d}" for (l, e) in self.local_experts.keys()])
            return web.json_response({
                "status": "ok",
                "expert_id": expert_id,
                "layer_id": layer_id,
                "size_mb": round(size_mb, 1),
                "loaded_count": len(self.local_experts),
                "local_experts": local_loaded,
            })
        except Exception as e:
            return web.json_response({"error": str(e)}, status=500)

    async def _http_handler_unload_expert(self, request: web.Request) -> web.Response:
        try:
            body = await request.json()
            expert_id = body.get("expert_id")
            layer_id = body.get("layer_id")
            if expert_id is None or layer_id is None:
                return web.json_response({"error": f"expert_id and layer_id are required, got expert_id={expert_id!r}, layer_id={layer_id!r}"}, status=400)
            expert_id = int(expert_id)
            layer_id = int(layer_id)
            self.unload_expert(expert_id, layer_id)
            local_loaded = sorted([f"L{l:02d}_E{e:03d}" for (l, e) in self.local_experts.keys()])
            return web.json_response({
                "status": "ok",
                "expert_id": expert_id,
                "layer_id": layer_id,
                "loaded_count": len(self.local_experts),
                "local_experts": local_loaded,
            })
        except Exception as e:
            return web.json_response({"error": str(e)}, status=500)

    async def _http_handler_load_expert_to_worker(self, request: web.Request) -> web.Response:
        try:
            body = await request.json()
            worker_id = body.get("worker_id")
            expert_id = body.get("expert_id")
            layer_id = body.get("layer_id")
            if not worker_id or expert_id is None or layer_id is None:
                return web.json_response({"error": "worker_id, expert_id, layer_id are required"}, status=400)
            if worker_id not in self.workers:
                return web.json_response({"error": f"Worker '{worker_id}' not found"}, status=404)
            w = self.workers[worker_id]
            file_path = body.get("file_path") or self._expert_file_path(int(expert_id), int(layer_id))
            async with self._session.post(
                f"http://{w.host}:{w.http_port}/load",
                json={"expert_id": int(expert_id), "layer_id": int(layer_id), "file_path": file_path},
                timeout=aiohttp.ClientTimeout(total=300),
            ) as resp:
                result = await resp.json()
            if resp.status != 200:
                return web.json_response(result, status=resp.status)
            return web.json_response({"status": "ok", "worker_id": worker_id, "expert_id": int(expert_id), "layer_id": int(layer_id), "worker_response": result})
        except Exception as e:
            return web.json_response({"error": str(e)}, status=500)

    async def _http_handler_unload_expert_from_worker(self, request: web.Request) -> web.Response:
        try:
            body = await request.json()
            worker_id = body.get("worker_id")
            expert_id = body.get("expert_id")
            layer_id = body.get("layer_id")
            if not worker_id or expert_id is None or layer_id is None:
                return web.json_response({"error": "worker_id, expert_id, layer_id are required"}, status=400)
            if worker_id not in self.workers:
                return web.json_response({"error": f"Worker '{worker_id}' not found"}, status=404)
            w = self.workers[worker_id]
            async with self._session.post(
                f"http://{w.host}:{w.http_port}/unload",
                json={"expert_id": int(expert_id), "layer_id": int(layer_id)},
                timeout=aiohttp.ClientTimeout(total=300),
            ) as resp:
                result = await resp.json()
            if resp.status != 200:
                return web.json_response(result, status=resp.status)
            return web.json_response({"status": "ok", "worker_id": worker_id, "expert_id": int(expert_id), "layer_id": int(layer_id), "worker_response": result})
        except Exception as e:
            return web.json_response({"error": str(e)}, status=500)

    async def _http_handler_inference(self, request: web.Request) -> web.Response:
        try:
            body = await request.json()
            prompt = body.get("prompt", "今天天气真好，")
            max_tokens = body.get("max_tokens", 20)

            if not hasattr(self, "_tokenizer"):
                from transformers import AutoTokenizer
                self._tokenizer = AutoTokenizer.from_pretrained(
                    "models/PaddlePaddle/ERNIE-4.5-21B-A3B-PT",
                    trust_remote_code=True,
                )
            tokenizer = self._tokenizer
            tokens = tokenizer(prompt, return_tensors="pt")
            input_ids = tokens["input_ids"]

            logger.info(f"Inference: prompt='{prompt}', tokens={input_ids.shape[1]}")
            logger.info(f"  Routing state: _local_routing={len(self._local_routing)} entries, workers={len(self.workers)}")
            result_ids = await self.generate(input_ids, max_new_tokens=max_tokens)
            result_text = tokenizer.decode(result_ids, skip_special_tokens=True)

            return web.json_response({
                "prompt": prompt,
                "input_tokens": input_ids.shape[1],
                "output_tokens": len(result_ids),
                "result": result_text,
            })
        except Exception as e:
            import traceback
            return web.json_response({"error": str(e), "trace": traceback.format_exc()}, status=500)

    async def _http_handler_status(self, request: web.Request) -> web.Response:
        local_loaded = sorted([f"L{l:02d}_E{e:03d}" for (l, e) in self.local_experts.keys()])
        return web.json_response({
            "nexus": "master",
            "http_port": self.http_port,
            "local_experts": local_loaded,
            "local_expert_count": len(self.local_experts),
            "workers": len(self.workers),
            "config": self.cfg,
        })

    async def _start_server(self):
        app = web.Application()
        app.router.add_get("/status", self._http_handler_status)
        app.router.add_post("/inference", self._http_handler_inference)
        app.router.add_post("/load_expert", self._http_handler_load_expert)
        app.router.add_post("/unload_expert", self._http_handler_unload_expert)
        app.router.add_post("/load_expert_to_worker", self._http_handler_load_expert_to_worker)
        app.router.add_post("/unload_expert_from_worker", self._http_handler_unload_expert_from_worker)
        app.router.add_post("/workers", self._http_handler_worker_register)
        app.router.add_get("/workers", self._http_handler_workers)
        app.router.add_get("/workers/{worker_id}/status", self._http_handler_worker_status)
        app.router.add_post("/workers/{worker_id}/load", self._http_handler_worker_load)
        app.router.add_post("/workers/{worker_id}/unload", self._http_handler_worker_unload)

        self._session = aiohttp.ClientSession()

        self._runner = web.AppRunner(app)
        await self._runner.setup()
        self._site = web.TCPSite(self._runner, self.bind_host, self.http_port)
        await self._site.start()

        logger.info(f"Nexus Master ready. HTTP={self.http_port}")
        logger.info(f"  Workers connect to: http://<master_ip>:{self.http_port}/register")
        logger.info(f"  Local experts: {len(self.local_experts)}")

        self._shutdown_event = asyncio.Event()
        await self._shutdown_event.wait()

    def start_blocking(self):
        try:
            asyncio.run(self._start_server())
        except KeyboardInterrupt:
            logger.info("Nexus Master shutting down")

    async def run_inference(self, input_ids: torch.Tensor, max_new_tokens: int = 20, eos_token_id: int = 2) -> list[int]:
        return await self.generate(input_ids, max_new_tokens, eos_token_id)


def main():
    import argparse
    parser = argparse.ArgumentParser(description="MoEAllocator Nexus Master")
    parser.add_argument("--manifest", "-m", type=str,
        default="output/splits/ERNIE-4.5-21B-A3B-PT-k6/manifest.json")
    parser.add_argument("--port", "-p", type=int, default=5000, help="HTTP port")
    parser.add_argument("--host", type=str, default="127.0.0.1", help="Bind host (default: 127.0.0.1)")
    parser.add_argument("--experts", "-e", type=str, default="",
        help="Comma-separated expert IDs to load locally (empty = no experts loaded)")
    parser.add_argument("--dtype", "-d", type=str, default="float32",
        choices=["float32", "fp16", "float16", "bf16", "bfloat16"],
        help="Weight precision: float32 (default), fp16/float16, bf16/bfloat16")
    parser.add_argument("--log-file", type=str, default="",
        help="Log file path (default: stdout only)")
    parser.add_argument("--kv-cache", action="store_true",
        help="Enable KV cache for faster generation (experimental)")
    args = parser.parse_args()

    if args.log_file:
        Path(args.log_file).parent.mkdir(parents=True, exist_ok=True)
        fh = logging.FileHandler(args.log_file)
        fh.setFormatter(logging.Formatter('%(asctime)s [%(levelname)s] %(name)s: %(message)s'))
        logging.getLogger().addHandler(fh)

    dtype_map = {"float32": torch.float32, "fp16": torch.float16, "float16": torch.float16,
                  "bf16": torch.bfloat16, "bfloat16": torch.bfloat16}
    dtype = dtype_map[args.dtype]
    expert_ids = [int(x) for x in args.experts.split(",")] if args.experts.strip() else []
    master = NexusMaster(args.manifest, http_port=args.port, bind_host=args.host,
                         local_expert_ids=expert_ids, dtype=dtype, kv_cache=args.kv_cache)
    print("Loading model...")
    master.load(expert_ids=expert_ids if expert_ids else None)
    print(f"Model loaded. Local experts: {len(master.local_experts)}")
    master.start_blocking()


if __name__ == "__main__":
    main()
