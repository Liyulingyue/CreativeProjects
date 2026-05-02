from __future__ import annotations

import torch
import torch.nn.functional as F


def _grouped_mm_fallback(input_tensor: torch.Tensor, weight: torch.Tensor, offsets: torch.Tensor) -> torch.Tensor:
    # grouped_mm 在部分 CPU/小矩阵场景会有 stride 对齐限制；这里保留逐组 mm，
    # 只作为兼容兜底，正常 ERNIE 维度会走 PyTorch grouped_mm。
    output = torch.zeros(input_tensor.size(0), weight.size(2), device=input_tensor.device, dtype=input_tensor.dtype)
    start = 0
    for expert_id, end in enumerate(offsets.tolist()):
        if start != end:
            torch.mm(input_tensor[start:end], weight[expert_id], out=output[start:end])
        start = end
    return output


def _grouped_linear(input_tensor: torch.Tensor, weight: torch.Tensor, offsets: torch.Tensor) -> torch.Tensor:
    # Transformers 的 expert 权重是 (expert, out, in)，grouped_mm 需要转成
    # (expert, in, out) 才能按每个 expert 的 offset 分段执行矩阵乘。
    grouped_input = input_tensor.to(weight.dtype)
    transposed = weight.transpose(-2, -1)
    try:
        if hasattr(F, "grouped_mm"):
            return F.grouped_mm(grouped_input, transposed, offs=offsets)
        if hasattr(torch, "_grouped_mm"):
            return torch._grouped_mm(grouped_input, transposed, offs=offsets)
    except RuntimeError:
        pass
    return _grouped_mm_fallback(grouped_input, transposed, offsets)


def grouped_moe_forward(
    hidden_states: torch.Tensor,
    top_k_index: torch.Tensor,
    top_k_weights: torch.Tensor,
    gate_up_proj: torch.Tensor,
    down_proj: torch.Tensor,
    num_experts: int,
) -> torch.Tensor:
    # 与 Transformers grouped_mm_experts_forward 保持同一条数值路径：
    # 展平 token/top-k 对，按 expert 排序后分组计算，再恢复原顺序聚合。
    device = hidden_states.device
    num_top_k = top_k_index.size(-1)
    num_tokens = hidden_states.size(0)
    hidden_dim = hidden_states.size(-1)

    token_idx = torch.arange(num_tokens, device=device).unsqueeze(1).expand(-1, num_top_k).reshape(-1)
    sample_weights = top_k_weights.reshape(-1)
    expert_ids = top_k_index.reshape(-1)

    invalid_mask = expert_ids >= num_experts
    expert_ids = expert_ids.clamp(0, num_experts - 1)

    # grouped_mm 要求同一个 expert 的样本连续排列；inv_perm 用于算完后还原
    # 到原始的 (token, top-k-rank) 顺序。
    perm = torch.argsort(expert_ids)
    inv_perm = torch.empty_like(perm)
    inv_perm[perm] = torch.arange(perm.size(0), device=device)

    expert_ids_grouped = expert_ids[perm]
    sample_weights_grouped = sample_weights[perm]
    selected_hidden_states = hidden_states[token_idx[perm]]

    histc_input = expert_ids_grouped.float() if device.type == "cpu" else expert_ids_grouped.int()
    tokens_per_expert = torch.histc(histc_input, bins=num_experts, min=0, max=num_experts - 1)
    offsets = torch.cumsum(tokens_per_expert, dim=0, dtype=torch.int32)

    # gate/up 使用和 Transformers 一致的 packed gate_up_proj，先一次 grouped
    # linear 得到两路投影，再做 SwiGLU 和 down_proj。
    gate, up = _grouped_linear(selected_hidden_states, gate_up_proj, offsets).chunk(2, dim=-1)
    projected = F.silu(gate) * up
    projected = _grouped_linear(projected, down_proj, offsets)

    weighted_out = projected * sample_weights_grouped.unsqueeze(-1)
    weighted_out.masked_fill_(invalid_mask[perm].unsqueeze(-1), 0.0)
    weighted_out = weighted_out[inv_perm]

    # 不用 index_add_，而是 reshape 后按 top-k 维度求和；这和 Transformers
    # grouped backend 的聚合顺序一致，也是本次 fp16 对齐的关键点。
    final_hidden_states = weighted_out.view(num_tokens, num_top_k, hidden_dim).sum(dim=1)
    return final_hidden_states.to(hidden_states.dtype)
