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

from src.moe_grouped import grouped_moe_forward
from src.nexus.startup import format_ids, log_kv, log_runtime_info, path_status

logging.basicConfig(level=logging.INFO,
                    format='%(asctime)s [%(levelname)s] %(name)s: %(message)s')
logger = logging.getLogger("nexus")


DTYPE_TO_CODE = {torch.float32: 0, torch.float16: 1, torch.bfloat16: 2}
CODE_TO_DTYPE = {0: torch.float32, 1: torch.float16, 2: torch.bfloat16}


def _tensor_to_wire_bytes(tensor: torch.Tensor, dtype: torch.dtype) -> bytes:
    tensor = tensor.detach().cpu().contiguous().to(dtype)
    if dtype == torch.bfloat16:
        return tensor.view(torch.uint16).numpy().tobytes()
    return tensor.numpy().tobytes()


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

        with open(manifest_path) as f:
            data = json.load(f)
        self.manifest = data
        self.cfg = data["config"]
        self._merge_original_config(data.get("model_path"))
        self.output_dir = Path(data["output_dir"])

        self.dtype = dtype
        self.kv_cache = kv_cache
        self._past_keys: dict[int, torch.Tensor] = {}
        self._past_values: dict[int, torch.Tensor] = {}
        self._routing_table: dict[tuple[int, int], object] = {}
        self._position_offset: int = 0

        self.hidden_size = self.cfg["hidden_size"]
        self.num_attention_heads = self.cfg.get("num_attention_heads", 20)
        self.num_kv_heads = self.cfg.get("num_key_value_heads", 4)
        self.head_dim = self.hidden_size // self.num_attention_heads
        self.num_hidden_layers = self.cfg["num_layers"]
        self.moe_k = self.cfg["moe_k"]
        self.moe_norm_min = self.cfg.get("moe_norm_min", 1e-12)
        self.num_experts = self.cfg.get("num_experts", self.cfg.get("moe_num_experts", 0))
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

    def _merge_original_config(self, model_path: str | None) -> None:
        if not model_path:
            return
        config_path = Path(model_path) / "config.json"
        if not config_path.exists():
            return
        with open(config_path) as f:
            original_config = json.load(f)

        mapped_keys = {
            "moe_num_experts": "num_experts",
            "num_hidden_layers": "num_layers",
            "moe_num_shared_experts": "num_shared_experts",
        }
        for source_key, target_key in mapped_keys.items():
            if target_key not in self.cfg and source_key in original_config:
                self.cfg[target_key] = original_config[source_key]

        for key in (
            "torch_dtype",
            "num_attention_heads",
            "num_key_value_heads",
            "intermediate_size",
            "rope_theta",
            "rms_norm_eps",
            "vocab_size",
            "moe_norm_min",
            "moe_layer_start_index",
            "moe_layer_end_index",
            "moe_layer_interval",
        ):
            if key not in self.cfg and key in original_config:
                self.cfg[key] = original_config[key]

    def load_backbone(self):
        logger.info("Loading backbone...")
        backbone_path = self.output_dir / "backbone" / "backbone.safetensors"
        w = storch.load_file(str(backbone_path))
        for k, v in w.items():
            self.weights[k] = self._to_tensor(k, v)
        logger.info(f"  {len(w)} backbone tensors")

        gate_path = self.output_dir / "backbone" / "gate.safetensors"
        if gate_path.exists():
            gw = storch.load_file(str(gate_path))
            for k, v in gw.items():
                self.weights[k] = self._to_tensor(k, v)
            logger.info(f"  {len(gw)} gate tensors")

        shared_path = self.output_dir / "backbone" / "shared_expert.safetensors"
        if shared_path.exists():
            shared = storch.load_file(str(shared_path))
            for k, v in shared.items():
                self.weights[k] = self._to_tensor(k, v)
            logger.info(f"  {len(shared)} shared expert tensors")

    def load_local_experts(self, expert_ids: list[int]):
        logger.info(f"Loading local experts {expert_ids}...")
        for eid in expert_ids:
            for lid in range(1, self.num_hidden_layers + 1):
                fp = self.output_dir / "experts" / f"expert_{eid:03d}_layer_{lid:03d}.safetensors"
                if fp.exists():
                    w = storch.load_file(str(fp))
                    self.local_experts[(lid, eid)] = {k: self._to_tensor(k, v) for k, v in w.items()}
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
        self.local_experts[(layer_id, expert_id)] = {k: self._to_tensor(k, v) for k, v in w.items()}
        self._local_routing.add((layer_id, expert_id))
        size_mb = sum(t.numel() * 4 for t in w.values()) / (1 << 20)
        return size_mb

    def _should_keep_fp32(self, name: str) -> bool:
        keep = self.cfg.get("keep_in_fp32", ["gate.weight", "moe_statics"])
        return any(pattern in name for pattern in keep)

    def _to_tensor(self, name: str, tensor: torch.Tensor) -> torch.Tensor:
        target_dtype = torch.float32 if self._should_keep_fp32(name) else self.dtype
        return tensor.to(target_dtype)

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
        return weight * x.to(input_dtype)

    def _rotate_half(self, x: torch.Tensor) -> torch.Tensor:
        """Rotates half the hidden dims of the input."""
        x1 = x[..., 0::2]
        x2 = x[..., 1::2]
        return torch.stack((-x2, x1), dim=-1).flatten(-2)

    def _rope_compute(self, seq_len, device, position_offset=0):
        inv_freq = 1.0 / (self.rope_theta ** (torch.arange(0, self.head_dim, 2, dtype=torch.float32, device=device) / self.head_dim))
        t = torch.arange(position_offset, position_offset + seq_len, dtype=torch.float32, device=device)
        freqs = torch.outer(t, inv_freq)
        emb = torch.cat((freqs, freqs), dim=-1)
        cos = emb.cos()
        sin = emb.sin()
        return cos, sin

    def _apply_rope(self, q, k, cos, sin):
        """Applies Rotary Position Embedding to the query and key tensors."""
        cos = cos[..., : cos.shape[-1] // 2].repeat_interleave(2, dim=-1)
        sin = sin[..., : sin.shape[-1] // 2].repeat_interleave(2, dim=-1)
        q_embed = (q.float() * cos) + (self._rotate_half(q).float() * sin)
        k_embed = (k.float() * cos) + (self._rotate_half(k).float() * sin)
        return q_embed.to(q.dtype), k_embed.to(k.dtype)

    def _attention(self, x, layer_id: int, use_cache: bool = False, position_offset: int = 0):
        assert x.dim() == 3 and x.shape[0] == 1 and x.shape[2] == self.hidden_size, f"Bad x shape: {x.shape}"
        prefix = f"model.layers.{layer_id}.self_attn"
        q_w = self.weights[f"{prefix}.q_proj.weight"]
        k_w = self.weights[f"{prefix}.k_proj.weight"]
        v_w = self.weights[f"{prefix}.v_proj.weight"]
        q = torch.matmul(x, q_w.t()) if q_w.shape[0] != x.shape[-1] else F.linear(x, q_w)
        k = torch.matmul(x, k_w.t()) if k_w.shape[0] != x.shape[-1] else F.linear(x, k_w)
        v = torch.matmul(x, v_w.t()) if v_w.shape[0] != x.shape[-1] else F.linear(x, v_w)

        bsz, seq_len, _ = x.shape
        q = q.reshape(bsz, seq_len, self.num_attention_heads, self.head_dim).transpose(1, 2)
        k_new = k.reshape(bsz, seq_len, self.num_kv_heads, self.head_dim).transpose(1, 2)
        v_new = v.reshape(bsz, seq_len, self.num_kv_heads, self.head_dim).transpose(1, 2)

        cos, sin = self._rope_compute(seq_len, x.device, position_offset)
        cos = cos.unsqueeze(0).unsqueeze(1)
        sin = sin.unsqueeze(0).unsqueeze(1)
        q, k_new = self._apply_rope(q, k_new, cos, sin)

        if use_cache and self.kv_cache:
            if layer_id in self._past_keys:
                k_new = torch.cat([self._past_keys[layer_id], k_new], dim=2)
                v_new = torch.cat([self._past_values[layer_id], v_new], dim=2)
            self._past_keys[layer_id] = k_new
            self._past_values[layer_id] = v_new

        if not (use_cache and self.kv_cache and seq_len > 1):
            try:
                attn_output = F.scaled_dot_product_attention(
                    q,
                    k_new,
                    v_new,
                    scale=self.head_dim ** -0.5,
                    is_causal=seq_len > 1,
                    enable_gqa=self.num_attention_heads != self.num_kv_heads,
                )
                attn_output = attn_output.transpose(1, 2).contiguous().view(bsz, seq_len, self.hidden_size)
                return F.linear(attn_output, self.weights[f"{prefix}.o_proj.weight"])
            except TypeError:
                pass

        if self.num_attention_heads != self.num_kv_heads:
            n_rep = self.num_attention_heads // self.num_kv_heads
            k_new = k_new.repeat_interleave(n_rep, dim=1)
            v_new = v_new.repeat_interleave(n_rep, dim=1)
        else:
            k_new = k_new.transpose(1, 2)
            v_new = v_new.transpose(1, 2)

        scaling = self.head_dim ** -0.5
        attn = torch.matmul(q, k_new.transpose(-2, -1)) * scaling
        if seq_len > 1:
            past_len = 0 if layer_id not in self._past_keys else (self._past_keys[layer_id].shape[2] - seq_len)
            total_len = past_len + seq_len
            causal = torch.triu(torch.ones(seq_len, total_len, dtype=torch.bool, device=x.device), diagonal=1)
            attn = attn.masked_fill(causal, float("-inf"))
        attn = F.softmax(attn, dim=-1, dtype=torch.float32).to(q.dtype)
        out = torch.matmul(attn, v_new).transpose(1, 2).contiguous().view(bsz, seq_len, self.hidden_size)
        return F.linear(out, self.weights[f"{prefix}.o_proj.weight"])

    def _ffn(self, x, down, up, gate):
        return F.linear(
            torch.nn.functional.silu(F.linear(x, gate)) * F.linear(x, up),
            down
        )

    def _gate_forward(self, x, layer_id: int):
        gate_weight = self.weights[f"model.layers.{layer_id}.mlp.gate.weight"]
        return F.linear(x.float(), gate_weight.float())

    def _gate_with_bias(self, x, layer_id: int):
        gate_weight = self.weights[f"model.layers.{layer_id}.mlp.gate.weight"]
        router_logits = F.linear(x.float(), gate_weight.float())
        routing_weights = F.softmax(router_logits, dim=-1, dtype=torch.float)
        
        # Add e_score_correction_bias
        bias_key = f"model.layers.{layer_id}.mlp.moe_statics.e_score_correction_bias"
        if bias_key in self.weights:
            routing_weights = routing_weights + self.weights[bias_key]
        
        return router_logits, routing_weights

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

    def _moe_expert_local_packed(self, x, layer_id: int, expert_id: int):
        key = (layer_id, expert_id)
        if key not in self.local_experts:
            return None
        w = self.local_experts[key]
        down = up = gate = None
        for k, v in w.items():
            if k.endswith("down_proj.weight"):
                down = v
            elif k.endswith("up_proj.weight"):
                up = v
            elif k.endswith("gate_proj.weight"):
                gate = v
        if None in (down, up, gate):
            return None
        gate_up = torch.cat((gate, up), dim=0)
        gate_out, up_out = F.linear(x, gate_up).chunk(2, dim=-1)
        return F.linear(torch.nn.functional.silu(gate_out) * up_out, down)

    def _pack_local_layer_experts(self, layer_id: int) -> tuple[torch.Tensor, torch.Tensor] | None:
        if not self.num_experts:
            return None
        # Master 只有在某一层的全部专家都在本地时才走 grouped-mm；
        # 分布式派发或只加载部分专家时，继续使用后面的路由/dispatch 逻辑。
        gate_up_proj = None
        down_proj = None
        for expert_id in range(self.num_experts):
            weights = self.local_experts.get((layer_id, expert_id))
            if weights is None:
                return None
            down = up = gate = None
            for k, v in weights.items():
                if k.endswith("down_proj.weight"):
                    down = v
                elif k.endswith("up_proj.weight"):
                    up = v
                elif k.endswith("gate_proj.weight"):
                    gate = v
            if None in (down, up, gate):
                return None
            if gate_up_proj is None:
                # 与 Transformers packed 权重保持一致：gate_proj 在前，up_proj 在后。
                gate_up_proj = torch.empty(
                    self.num_experts,
                    gate.shape[0] + up.shape[0],
                    gate.shape[1],
                    dtype=gate.dtype,
                    device=gate.device,
                )
                down_proj = torch.empty(
                    self.num_experts,
                    down.shape[0],
                    down.shape[1],
                    dtype=down.dtype,
                    device=down.device,
                )
            gate_up_proj[expert_id, : gate.shape[0]].copy_(gate)
            gate_up_proj[expert_id, gate.shape[0] :].copy_(up)
            down_proj[expert_id].copy_(down)
        if gate_up_proj is None or down_proj is None:
            return None
        return gate_up_proj, down_proj

    def _moe_experts_grouped_local(
        self,
        x_flat: torch.Tensor,
        layer_id: int,
        top_k_index: torch.Tensor,
        top_k_weights: torch.Tensor,
    ) -> torch.Tensor | None:
        packed = self._pack_local_layer_experts(layer_id)
        if packed is None:
            return None
        gate_up_proj, down_proj = packed
        return grouped_moe_forward(x_flat, top_k_index, top_k_weights, gate_up_proj, down_proj, self.num_experts)

    async def _dispatch_expert(self, worker: WorkerInfo, layer_id: int, expert_id: int, hidden: torch.Tensor, rank: int = 0):
        try:
            logger.info(f"  -> dispatch expert_{expert_id}_L{layer_id} to {worker.worker_id}")
            reader, writer = await asyncio.open_connection(worker.host, worker.tcp_port)
            dtype_code = DTYPE_TO_CODE.get(self.dtype, 0)
            result_dtype = CODE_TO_DTYPE.get(dtype_code, torch.float32)
            header = struct.pack("!IIIIII", layer_id, expert_id, hidden.shape[0], hidden.shape[1], hidden.shape[2], dtype_code)
            data = _tensor_to_wire_bytes(hidden, self.dtype)
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
            result = torch.frombuffer(result_data, dtype=result_dtype).reshape(hidden.shape).clone()
            logger.debug(f"  <- expert_{expert_id}_L{layer_id} result={result.shape}")
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
        bsz, seq_len, _ = x_norm.shape
        x_flat = x_norm.view(-1, x_norm.shape[-1])
        shared_output = self._shared_expert(x_flat, layer_id)
        if layer_id == 1:
            logger.info(f"[DBG] L{layer_id} shared_expert: sum={shared_output.sum().item():.6f}, first5={shared_output[0, :5].tolist()}")

        # Routing
        gate_weight = self.weights[f"model.layers.{layer_id}.mlp.gate.weight"]
        router_logits = F.linear(x_flat.float(), gate_weight.float())
        routing_weights = F.softmax(router_logits, dim=-1, dtype=torch.float)
        
        # Add bias for selection only
        bias_key = f"model.layers.{layer_id}.mlp.moe_statics.e_score_correction_bias"
        routing_for_selection = routing_weights
        if bias_key in self.weights:
            routing_for_selection = routing_weights + self.weights[bias_key]
        
        num_experts = router_logits.shape[-1]
        K = min(self.moe_k, num_experts)

        _, top_k_index = torch.topk(routing_for_selection, K, dim=-1)
        top_k_weights = torch.gather(routing_weights, dim=-1, index=top_k_index)
        top_k_weights = top_k_weights / torch.clamp(top_k_weights.sum(dim=-1, keepdim=True), min=self.moe_norm_min)
        top_k_weights = top_k_weights.to(x_flat.dtype)
        num_tokens = routing_weights.shape[0]

        # 全本地专家场景下直接使用 grouped-mm，避免 per-expert index_add_
        # 带来的 fp16 聚合顺序差异。
        routed_output = self._moe_experts_grouped_local(x_flat, layer_id, top_k_index, top_k_weights)
        if routed_output is not None:
            logger.info(f"  Layer {layer_id}: grouped local experts={self.num_experts}, total={num_tokens * K}")
            return (routed_output + shared_output).view(bsz, seq_len, self.hidden_size)

        routing = self._build_routing_table()

        shown = min(2, num_tokens)
        # Log first token's top-k selection
        logger.info(f"  Layer {layer_id}: _local_routing={len(self._local_routing)}, workers={list(self.workers.keys())}")
        logger.info(f"  Layer {layer_id}: first {shown} token(s) gate picks: {top_k_index[:shown].tolist()}")

        tasks = {}
        local_exec = 0
        dispatched = 0
        fallback_total = 0
        routed_output = torch.zeros_like(x_flat)

        local_by_expert: dict[int, list[tuple[int, torch.Tensor]]] = {}

        for tok_idx in range(num_tokens):
            selected = []
            used_expert_ids = set()
            for rank, eid in enumerate(top_k_index[tok_idx].tolist()):
                key = (layer_id, eid)
                if key in self._local_routing:
                    selected.append((eid, "local", top_k_weights[tok_idx, rank], rank))
                    used_expert_ids.add(eid)
                elif key in routing:
                    selected.append((eid, "dispatch", top_k_weights[tok_idx, rank], rank))
                    used_expert_ids.add(eid)

            if len(selected) < K:
                sorted_idx_by_prob = torch.argsort(routing_for_selection[tok_idx], descending=True)
                for eid in sorted_idx_by_prob.tolist():
                    if len(selected) >= K:
                        break
                    if eid in used_expert_ids:
                        continue
                    key = (layer_id, eid)
                    if key in self._local_routing:
                        prob = routing_weights[tok_idx, eid].to(x_flat.dtype)
                        selected.append((eid, "fallback_local", prob, len(selected)))
                        used_expert_ids.add(eid)
                        fallback_total += 1
                    elif key in routing:
                        prob = routing_weights[tok_idx, eid].to(x_flat.dtype)
                        selected.append((eid, "fallback_dispatch", prob, len(selected)))
                        used_expert_ids.add(eid)
                        fallback_total += 1

            # Normalize fallback probabilities only if top-k had to be replaced by available routes.
            selected_probs = torch.stack([prob for _, _, prob, _ in selected]) if selected else x_flat.new_empty((0,))
            norm = selected_probs.sum().clamp(min=self.moe_norm_min)

            if tok_idx == 0 and layer_id == 1:
                logger.info(f"[DBG] L{layer_id} tok {tok_idx}: selected={[(e, s) for e, s, _, _ in selected]}, weights={[round((p/norm).item(), 4) for _, _, p, _ in selected]}")

            for rank, (expert_id, source, prob, _top_k_pos) in enumerate(selected):
                weight = (prob / norm).to(x_norm.dtype)

                key = (layer_id, expert_id)
                if source in ("local", "fallback_local"):
                    local_by_expert.setdefault(expert_id, []).append((tok_idx, weight))
                else:
                    worker = routing[key]
                    task = asyncio.create_task(
                        self._dispatch_expert(worker, layer_id, expert_id, x_flat[tok_idx:tok_idx + 1].view(1, 1, -1), rank=rank)
                    )
                    tasks[(tok_idx, rank)] = (expert_id, task, weight)
                    dispatched += 1

        for expert_id in sorted(local_by_expert):
            pairs = local_by_expert[expert_id]
            token_idx = torch.tensor([token for token, _ in pairs], dtype=torch.long, device=x_flat.device)
            current_weights = torch.stack([weight for _, weight in pairs]).to(x_flat.dtype)
            current_state = x_flat[token_idx]
            current_hidden_states = self._moe_expert_local_packed(current_state, layer_id, expert_id)
            if current_hidden_states is None:
                continue
            current_hidden_states = current_hidden_states * current_weights[:, None]
            routed_output.index_add_(0, token_idx, current_hidden_states.to(routed_output.dtype))
            local_exec += len(pairs)

        if tasks:
            task_list = [t for _, t, _ in tasks.values()]
            results = await asyncio.gather(*task_list, return_exceptions=True)
            for (tok_idx, rank), result in zip(tasks.keys(), results):
                if isinstance(result, torch.Tensor):
                    weight = tasks[(tok_idx, rank)][2]
                    expert_id = tasks[(tok_idx, rank)][0]
                    if tok_idx == 0 and layer_id == 1:
                        logger.info(f"[DBG]   expert {expert_id} (dispatch): sum={result.sum().item():.6f}, first5={result[0, 0, :5].tolist()}")
                    routed_output[tok_idx:tok_idx + 1] = routed_output[tok_idx:tok_idx + 1] + result.view(1, -1) * weight
                else:
                    logger.error(f"Expert {tasks[(tok_idx, rank)][0]} failed: {result}")

        logger.info(f"  Layer {layer_id}: local={local_exec}, dispatched={dispatched}, fallback={fallback_total}, total={num_tokens * K}")

        return (routed_output + shared_output).view(bsz, seq_len, self.hidden_size)

    async def _transformer_layer(self, x, layer_id: int, use_cache: bool = False, position_offset: int = 0):
        x_norm = self._rms_norm(x, self.weights[f"model.layers.{layer_id}.input_layernorm.weight"])
        attn_out = self._attention(x_norm, layer_id, use_cache, position_offset)
        x = x + attn_out

        x_norm = self._rms_norm(x, self.weights[f"model.layers.{layer_id}.post_attention_layernorm.weight"])
        if layer_id == 0:
            x = x + self._standard_mlp(x_norm, layer_id)
        else:
            moe_out = await self._moe_layer(x_norm, layer_id)
            x = x + moe_out
        if layer_id < 3:
            logger.info(f"[DBG] layer {layer_id} out: sum={x.sum().item():.6f}, first5={x[0, 0, :5].tolist()}")
        return x

    async def forward(self, input_ids: torch.Tensor, use_cache: bool = False, position_offset: int = 0) -> torch.Tensor:
        x = F.embedding(input_ids, self.weights["model.embed_tokens.weight"])
        for lid in range(self.num_hidden_layers):
            x = await self._transformer_layer(x, lid, use_cache, position_offset)
        x = self._rms_norm(x, self.weights["model.norm.weight"])
        return F.linear(x, self.weights["model.embed_tokens.weight"])

    @torch.no_grad()
    async def generate(self, input_ids: torch.Tensor, max_new_tokens: int = 50, eos_token_id: int = 2) -> list[int]:
        if self.kv_cache:
            self._past_keys.clear()
            self._past_values.clear()
        self._position_offset = 0
        generated = input_ids[0].tolist()

        for step in range(max_new_tokens):
            if self.kv_cache:
                if step == 0:
                    logits = await self.forward(input_ids, use_cache=True, position_offset=0)
                else:
                    new_id = torch.tensor([[generated[-1]]], dtype=input_ids.dtype)
                    logits = await self.forward(new_id, use_cache=True, position_offset=self._position_offset)
                self._position_offset += input_ids.shape[1] if step == 0 else 1
            else:
                full_ids = torch.tensor([generated], dtype=input_ids.dtype)
                logits = await self.forward(full_ids, use_cache=False, position_offset=0)

            next_token = logits[0, -1].argmax().item()
            generated.append(next_token)
            if next_token == eos_token_id:
                break

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
            "runtime_dtype": str(self.dtype).replace("torch.", ""),
            "model_config_dtype": self.cfg.get("torch_dtype"),
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
        logger.info(f"  Workers connect to: http://<master_ip>:{self.http_port}/workers")
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
        help="Runtime weight precision: float32 (default), fp16/float16, bf16/bfloat16")
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
    log_runtime_info(logger, "master")
    log_kv(
        logger,
        "master_config",
        [
            ("bind_host", args.host),
            ("http_port", args.port),
            ("runtime_dtype_arg", args.dtype),
            ("runtime_dtype_effective", str(master.dtype).replace("torch.", "")),
            ("kv_cache", args.kv_cache),
            ("manifest", path_status(args.manifest)),
            ("output_dir", path_status(master.output_dir)),
            ("local_expert_ids", format_ids(expert_ids)),
        ],
    )
    log_kv(
        logger,
        "model_info",
        [
            ("architecture", master.manifest.get("architecture", "unknown")),
            ("model_path", path_status(master.manifest.get("model_path", ""))),
            ("hidden_size", master.hidden_size),
            ("layers", master.num_hidden_layers),
            ("attention_heads", master.num_attention_heads),
            ("kv_heads", master.num_kv_heads),
            ("head_dim", master.head_dim),
            ("experts_per_layer", master.cfg.get("num_experts")),
            ("moe_top_k", master.moe_k),
            ("shared_experts", master.num_shared_experts),
            ("moe_intermediate", master.moe_intermediate),
            ("model_config_dtype", master.cfg.get("torch_dtype")),
        ],
    )
    logger.info("Loading model...")
    master.load(expert_ids=expert_ids if expert_ids else None)
    logger.info("Model loaded. local_expert_files=%s", len(master.local_experts))
    master.start_blocking()


if __name__ == "__main__":
    main()
