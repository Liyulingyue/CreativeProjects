#!/usr/bin/env python3
"""Compare ERNIE-4.5 MoE Transformers inference with MoEAllocator split inference.

This script is intentionally diagnostic: it feeds identical input IDs into the
original Hugging Face/Transformers model and the split `MiniMoE` implementation,
then reports logits, generated tokens, per-layer hidden-state differences, and
router choices when available.
"""

from __future__ import annotations

import argparse
import gc
import json
import platform
import time
from pathlib import Path
from typing import Any

import torch
from transformers import AutoModelForCausalLM, AutoTokenizer

from inference_mini_plus2 import MiniMoE


def _resolve_dtype(name: str) -> torch.dtype:
    mapping = {
        "auto": torch.bfloat16,
        "bf16": torch.bfloat16,
        "bfloat16": torch.bfloat16,
        "fp16": torch.float16,
        "float16": torch.float16,
        "fp32": torch.float32,
        "float32": torch.float32,
    }
    if name not in mapping:
        raise ValueError(f"Unsupported dtype: {name}")
    return mapping[name]


def _prepare_input(tokenizer: Any, prompt: str, use_chat_template: bool) -> torch.Tensor:
    if use_chat_template:
        text = tokenizer.apply_chat_template(
            [{"role": "user", "content": prompt}],
            tokenize=False,
            add_generation_prompt=True,
        )
        return tokenizer([text], add_special_tokens=False, return_tensors="pt")["input_ids"]
    return tokenizer(prompt, return_tensors="pt")["input_ids"]


def _topk(logits: torch.Tensor, k: int = 10) -> list[dict[str, float | int]]:
    values, indices = torch.topk(logits.float(), k=k, dim=-1)
    return [
        {"token_id": int(token_id), "logit": float(value)}
        for token_id, value in zip(indices.tolist(), values.tolist(), strict=True)
    ]


def _tensor_summary(tensor: torch.Tensor) -> dict[str, float | list[int]]:
    tensor = tensor.float()
    return {
        "shape": list(tensor.shape),
        "sum": float(tensor.sum()),
        "mean": float(tensor.mean()),
        "std": float(tensor.std()),
        "min": float(tensor.min()),
        "max": float(tensor.max()),
    }


def _diff_summary(left: torch.Tensor, right: torch.Tensor) -> dict[str, float | list[int]]:
    left = left.float()
    right = right.float()
    diff = (left - right).abs()
    denom = torch.maximum(left.abs(), right.abs()).clamp_min(1e-12)
    rel = diff / denom
    return {
        "shape": list(diff.shape),
        "max_abs": float(diff.max()),
        "mean_abs": float(diff.mean()),
        "rmse": float(torch.sqrt(torch.mean((left - right) ** 2))),
        "max_rel": float(rel.max()),
        "mean_rel": float(rel.mean()),
    }


def _read_json(path: str | Path) -> dict[str, Any]:
    with open(path, encoding="utf-8") as f:
        return json.load(f)


def _json_default(value: Any) -> Any:
    if isinstance(value, torch.Tensor):
        return value.detach().cpu().tolist()
    if isinstance(value, torch.dtype):
        return str(value).replace("torch.", "")
    if isinstance(value, Path):
        return str(value)
    raise TypeError(f"Object of type {type(value).__name__} is not JSON serializable")


def run_original(
    model_path: str,
    input_ids: torch.Tensor,
    dtype: torch.dtype,
    max_new_tokens: int,
    skip_generate: bool,
) -> dict[str, Any]:
    print("[original] loading Transformers model...")
    started_at = time.perf_counter()
    model = AutoModelForCausalLM.from_pretrained(
        model_path,
        dtype=dtype,
        low_cpu_mem_usage=True,
    )
    model.eval()
    load_seconds = time.perf_counter() - started_at

    layer_outputs: dict[int, torch.Tensor] = {}
    router_outputs: dict[int, dict[str, torch.Tensor]] = {}
    hooks = []

    def layer_hook(layer_id: int):
        def hook(_module: Any, _inputs: Any, output: Any) -> None:
            layer_outputs[layer_id] = output.detach().cpu().float()

        return hook

    def router_hook(layer_id: int):
        def hook(_module: Any, _inputs: Any, output: Any) -> None:
            router_logits, selected_experts, routing_weights = output
            router_outputs[layer_id] = {
                "router_logits": router_logits.detach().cpu().float(),
                "selected_experts": selected_experts.detach().cpu(),
                "routing_weights": routing_weights.detach().cpu().float(),
            }

        return hook

    for layer_id, layer in enumerate(model.model.layers):
        hooks.append(layer.register_forward_hook(layer_hook(layer_id)))
        if hasattr(layer.mlp, "gate"):
            hooks.append(layer.mlp.gate.register_forward_hook(router_hook(layer_id)))

    with torch.no_grad():
        forward_started_at = time.perf_counter()
        outputs = model(input_ids=input_ids, use_cache=False, logits_to_keep=0)
        forward_seconds = time.perf_counter() - forward_started_at
        logits = outputs.logits.detach().cpu().float()
        generate_seconds = 0.0
        generated = input_ids.detach().cpu()

    for hook in hooks:
        hook.remove()

    with torch.no_grad():
        if not skip_generate and max_new_tokens > 0:
            generate_started_at = time.perf_counter()
            generated = model.generate(input_ids=input_ids, max_new_tokens=max_new_tokens, do_sample=False)
            generate_seconds = time.perf_counter() - generate_started_at

    result = {
        "logits": logits,
        "generated_ids": generated.detach().cpu(),
        "last_token_topk": _topk(logits[0, -1]),
        "layer_outputs": layer_outputs,
        "router_outputs": router_outputs,
        "timing": {
            "load_seconds": load_seconds,
            "forward_seconds": forward_seconds,
            "generate_seconds": generate_seconds,
        },
    }

    del model
    gc.collect()
    return result


def run_split(
    manifest_path: str,
    input_ids: torch.Tensor,
    dtype: torch.dtype,
    max_new_tokens: int,
    expert_ids: list[int] | None,
    skip_generate: bool,
) -> dict[str, Any]:
    print("[split] loading split MiniMoE model...")
    started_at = time.perf_counter()
    model = MiniMoE(manifest_path, expert_ids=expert_ids, dtype=dtype)
    model.load_all()
    load_seconds = time.perf_counter() - started_at

    layer_outputs: dict[int, torch.Tensor] = {}
    router_outputs: dict[int, dict[str, torch.Tensor]] = {}

    original_transformer_layer = model._transformer_layer
    original_moe_layer = model._moe_layer

    def traced_moe_layer(x_norm: torch.Tensor, layer_id: int) -> torch.Tensor:
        gate_weight = model.weights[f"model.layers.{layer_id}.mlp.gate.weight"]
        router_logits = torch.nn.functional.linear(x_norm.float(), gate_weight.float())
        routing_weights = torch.nn.functional.softmax(router_logits, dim=-1, dtype=torch.float)
        selection = routing_weights
        bias_key = f"model.layers.{layer_id}.mlp.moe_statics.e_score_correction_bias"
        if bias_key in model.weights:
            selection = routing_weights + model.weights[bias_key]
        selected_experts = torch.topk(selection.view(-1, selection.shape[-1]), k=model.moe_k, dim=-1).indices
        selected_weights = torch.gather(routing_weights.view(-1, routing_weights.shape[-1]), -1, selected_experts)
        selected_weights = selected_weights / torch.clamp(
            selected_weights.sum(dim=-1, keepdim=True), min=model.moe_norm_min
        )
        router_outputs[layer_id] = {
            "router_logits": router_logits.detach().cpu().float(),
            "selected_experts": selected_experts.detach().cpu(),
            "routing_weights": selected_weights.detach().cpu().float(),
        }
        return original_moe_layer(x_norm, layer_id)

    def traced_transformer_layer(x: torch.Tensor, layer_id: int) -> torch.Tensor:
        out = original_transformer_layer(x, layer_id)
        layer_outputs[layer_id] = out.detach().cpu().float()
        return out

    model._moe_layer = traced_moe_layer  # type: ignore[method-assign]
    model._transformer_layer = traced_transformer_layer  # type: ignore[method-assign]

    with torch.no_grad():
        forward_started_at = time.perf_counter()
        logits = model.forward(input_ids).detach().cpu().float()
        forward_seconds = time.perf_counter() - forward_started_at
        generate_seconds = 0.0
        generated = input_ids.detach().cpu()

    model._moe_layer = original_moe_layer  # type: ignore[method-assign]
    model._transformer_layer = original_transformer_layer  # type: ignore[method-assign]

    with torch.no_grad():
        if not skip_generate and max_new_tokens > 0:
            generate_started_at = time.perf_counter()
            generated = torch.tensor([model.generate(input_ids.clone(), max_new_tokens=max_new_tokens)], dtype=torch.long)
            generate_seconds = time.perf_counter() - generate_started_at

    result = {
        "logits": logits,
        "generated_ids": generated,
        "last_token_topk": _topk(logits[0, -1]),
        "layer_outputs": layer_outputs,
        "router_outputs": router_outputs,
        "timing": {
            "load_seconds": load_seconds,
            "forward_seconds": forward_seconds,
            "generate_seconds": generate_seconds,
        },
    }

    del model
    gc.collect()
    return result


def build_report(
    original: dict[str, Any],
    split: dict[str, Any],
    tokenizer: Any,
    *,
    model_path: str,
    manifest_path: str,
    dtype_name: str,
    prompt: str,
    input_ids: torch.Tensor,
    expert_ids: list[int] | None,
    max_new_tokens: int,
    skip_generate: bool,
    use_chat_template: bool,
) -> dict[str, Any]:
    original_ids = original["generated_ids"][0].tolist()
    split_ids = split["generated_ids"][0].tolist()
    generated_compare_len = min(len(original_ids), len(split_ids))
    generated_mismatches = [
        {"position": idx, "original": int(original_ids[idx]), "split": int(split_ids[idx])}
        for idx in range(generated_compare_len)
        if original_ids[idx] != split_ids[idx]
    ]
    if len(original_ids) != len(split_ids):
        generated_mismatches.append(
            {"position": generated_compare_len, "original": len(original_ids), "split": len(split_ids), "kind": "length"}
        )

    final_logits = _diff_summary(original["logits"], split["logits"])
    last_token_logits = _diff_summary(original["logits"][:, -1, :], split["logits"][:, -1, :])
    original_top1 = original["last_token_topk"][0]["token_id"]
    split_top1 = split["last_token_topk"][0]["token_id"]

    layer_diffs = []
    for layer_id in sorted(set(original["layer_outputs"]) & set(split["layer_outputs"])):
        layer_diffs.append(
            {
                "layer": layer_id,
                "hidden_state_diff": _diff_summary(original["layer_outputs"][layer_id], split["layer_outputs"][layer_id]),
                "original_summary": _tensor_summary(original["layer_outputs"][layer_id]),
                "split_summary": _tensor_summary(split["layer_outputs"][layer_id]),
            }
        )

    router_diffs = []
    for layer_id in sorted(set(original["router_outputs"]) & set(split["router_outputs"])):
        orig_router = original["router_outputs"][layer_id]
        split_router = split["router_outputs"][layer_id]
        selected_match = orig_router["selected_experts"].eq(split_router["selected_experts"])
        router_diffs.append(
            {
                "layer": layer_id,
                "router_logits_diff": _diff_summary(orig_router["router_logits"], split_router["router_logits"]),
                "routing_weights_diff": _diff_summary(orig_router["routing_weights"], split_router["routing_weights"]),
                "selected_total": int(selected_match.numel()),
                "selected_mismatch": int(selected_match.numel() - selected_match.sum().item()),
                "first_token_original_selected": orig_router["selected_experts"][0].tolist(),
                "first_token_split_selected": split_router["selected_experts"][0].tolist(),
                "first_token_original_weights": [float(x) for x in orig_router["routing_weights"][0].float().tolist()],
                "first_token_split_weights": [float(x) for x in split_router["routing_weights"][0].float().tolist()],
            }
        )

    model_config_path = Path(model_path) / "config.json"
    manifest = _read_json(manifest_path)
    model_config = _read_json(model_config_path) if model_config_path.exists() else {}

    return {
        "summary": {
            "dtype": dtype_name,
            "model": model_path,
            "manifest": manifest_path,
            "prompt": prompt,
            "input_ids": input_ids.tolist(),
            "input_length": int(input_ids.shape[1]),
            "max_new_tokens": max_new_tokens,
            "skip_generate": skip_generate,
            "chat_template": use_chat_template,
            "experts": "all" if expert_ids is None else expert_ids,
            "final_logits": final_logits,
            "last_token_logits": last_token_logits,
            "last_token_top1_match": bool(original_top1 == split_top1),
            "last_token_original_top1": int(original_top1),
            "last_token_split_top1": int(split_top1),
            "generated_token_match": bool(not generated_mismatches),
            "generated_mismatch_count": len(generated_mismatches),
        },
        "environment": {
            "python": platform.python_version(),
            "platform": platform.platform(),
            "torch": torch.__version__,
        },
        "model_config": {
            "model_type": model_config.get("model_type"),
            "architectures": model_config.get("architectures"),
            "torch_dtype": model_config.get("torch_dtype"),
            "num_hidden_layers": model_config.get("num_hidden_layers"),
            "hidden_size": model_config.get("hidden_size"),
            "moe_num_experts": model_config.get("moe_num_experts"),
            "moe_k": model_config.get("moe_k"),
            "moe_num_shared_experts": model_config.get("moe_num_shared_experts"),
        },
        "split_manifest": {
            "output_dir": manifest.get("output_dir"),
            "expert_files": len(manifest.get("expert_files", [])),
            "config": manifest.get("config", {}),
        },
        "timing": {
            "original": original["timing"],
            "split": split["timing"],
        },
        "generation": {
            "original_ids": original_ids,
            "split_ids": split_ids,
            "mismatches": generated_mismatches,
            "original_text": tokenizer.decode(original_ids, skip_special_tokens=True),
            "split_text": tokenizer.decode(split_ids, skip_special_tokens=True),
        },
        "topk": {
            "original_last_token_top10": original["last_token_topk"],
            "split_last_token_top10": split["last_token_topk"],
        },
        "layer_diffs": layer_diffs,
        "router_diffs": router_diffs,
    }


def write_markdown_report(report: dict[str, Any], path: str | Path) -> None:
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    summary = report["summary"]
    lines = [
        "# ERNIE-4.5-21B-A3B-PT fp16 full-chain precision alignment",
        "",
        "## End-to-end result",
        "",
        f"- dtype: `{summary['dtype']}`",
        f"- model: `{summary['model']}`",
        f"- manifest: `{summary['manifest']}`",
        f"- experts: `{summary['experts']}`",
        f"- prompt: `{summary['prompt']}`",
        f"- input_length: `{summary['input_length']}`",
        f"- max_new_tokens: `{summary['max_new_tokens']}`",
        f"- generated_token_match: `{summary['generated_token_match']}`",
        f"- generated_mismatch_count: `{summary['generated_mismatch_count']}`",
        f"- last_token_top1_match: `{summary['last_token_top1_match']}`",
        "",
        "### Final logits",
        "",
        "| scope | max_abs | mean_abs | rmse | max_rel | mean_rel |",
        "|---|---:|---:|---:|---:|---:|",
    ]

    for label, key in (("all logits", "final_logits"), ("last token logits", "last_token_logits")):
        item = summary[key]
        lines.append(
            f"| {label} | {item['max_abs']:.8f} | {item['mean_abs']:.8f} | "
            f"{item['rmse']:.8f} | {item['max_rel']:.8f} | {item['mean_rel']:.8f} |"
        )

    lines.extend(
        [
            "",
            "### Greedy generation",
            "",
            f"- original_ids: `{report['generation']['original_ids']}`",
            f"- split_ids: `{report['generation']['split_ids']}`",
            f"- original_text: `{report['generation']['original_text']}`",
            f"- split_text: `{report['generation']['split_text']}`",
            "",
            "### Last-token top10",
            "",
            f"- original: `{report['topk']['original_last_token_top10']}`",
            f"- split: `{report['topk']['split_last_token_top10']}`",
            "",
            "## Per-layer hidden-state diffs",
            "",
            "| layer | max_abs | mean_abs | rmse | max_rel | mean_rel |",
            "|---:|---:|---:|---:|---:|---:|",
        ]
    )
    for item in report["layer_diffs"]:
        diff = item["hidden_state_diff"]
        lines.append(
            f"| {item['layer']} | {diff['max_abs']:.8f} | {diff['mean_abs']:.8f} | "
            f"{diff['rmse']:.8f} | {diff['max_rel']:.8f} | {diff['mean_rel']:.8f} |"
        )

    lines.extend(
        [
            "",
            "## Per-layer router diffs",
            "",
            "| layer | selected_mismatch/total | logits_max_abs | logits_mean_abs | weights_max_abs | weights_mean_abs | first_token_original | first_token_split |",
            "|---:|---:|---:|---:|---:|---:|---|---|",
        ]
    )
    for item in report["router_diffs"]:
        logits = item["router_logits_diff"]
        weights = item["routing_weights_diff"]
        lines.append(
            f"| {item['layer']} | {item['selected_mismatch']}/{item['selected_total']} | "
            f"{logits['max_abs']:.8f} | {logits['mean_abs']:.8f} | "
            f"{weights['max_abs']:.8f} | {weights['mean_abs']:.8f} | "
            f"`{item['first_token_original_selected']}` | `{item['first_token_split_selected']}` |"
        )

    lines.extend(
        [
            "",
            "## Runtime",
            "",
            "| path | load_s | forward_s | generate_s |",
            "|---|---:|---:|---:|",
        ]
    )
    for label in ("original", "split"):
        timing = report["timing"][label]
        lines.append(
            f"| {label} | {timing['load_seconds']:.3f} | {timing['forward_seconds']:.3f} | {timing['generate_seconds']:.3f} |"
        )

    lines.extend(
        [
            "",
            "## Model metadata",
            "",
            f"- model_config: `{report['model_config']}`",
            f"- split_manifest: `{report['split_manifest']}`",
            f"- environment: `{report['environment']}`",
            "",
        ]
    )
    path.write_text("\n".join(lines), encoding="utf-8")


def print_comparison(original: dict[str, Any], split: dict[str, Any], tokenizer: Any) -> None:
    original_logits = original["logits"]
    split_logits = split["logits"]
    diff = (original_logits - split_logits).abs()

    print("\n=== Final logits ===")
    print(f"max_abs_diff={diff.max().item():.8f}, mean_abs_diff={diff.mean().item():.8f}")
    print(f"original_top10={original['last_token_topk']}")
    print(f"split_top10   ={split['last_token_topk']}")

    print("\n=== Greedy generation ===")
    original_ids = original["generated_ids"][0].tolist()
    split_ids = split["generated_ids"][0].tolist()
    print(f"original_ids={original_ids}")
    print(f"split_ids   ={split_ids}")
    print(f"original_text={tokenizer.decode(original_ids, skip_special_tokens=True)!r}")
    print(f"split_text   ={tokenizer.decode(split_ids, skip_special_tokens=True)!r}")

    print("\n=== Layer hidden-state diffs ===")
    for layer_id in sorted(set(original["layer_outputs"]) & set(split["layer_outputs"])):
        layer_diff = (original["layer_outputs"][layer_id] - split["layer_outputs"][layer_id]).abs()
        print(
            f"L{layer_id:02d}: max={layer_diff.max().item():.8f}, "
            f"mean={layer_diff.mean().item():.8f}, "
            f"orig={_tensor_summary(original['layer_outputs'][layer_id])}, "
            f"split={_tensor_summary(split['layer_outputs'][layer_id])}"
        )

    print("\n=== Router first-token choices ===")
    for layer_id in sorted(set(original["router_outputs"]) & set(split["router_outputs"])):
        orig_router = original["router_outputs"][layer_id]
        split_router = split["router_outputs"][layer_id]
        print(
            f"L{layer_id:02d}: "
            f"orig_selected={orig_router['selected_experts'][0].tolist()} "
            f"split_selected={split_router['selected_experts'][0].tolist()} "
            f"orig_weights={[round(float(x), 6) for x in orig_router['routing_weights'][0].tolist()]} "
            f"split_weights={[round(float(x), 6) for x in split_router['routing_weights'][0].tolist()]}"
        )


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--model", default="models/PaddlePaddle/ERNIE-4.5-21B-A3B-PT")
    parser.add_argument("--manifest", default="output/splits/ERNIE-4.5-21B-A3B-PT/manifest.json")
    parser.add_argument("--prompt", default="What a nice day!")
    parser.add_argument("--max-new-tokens", type=int, default=1)
    parser.add_argument("--skip-generate", action="store_true", help="Only compare one forward pass/logits.")
    parser.add_argument("--dtype", choices=["auto", "bf16", "bfloat16", "fp16", "float16", "fp32", "float32"], default="auto")
    parser.add_argument("--chat-template", action="store_true")
    parser.add_argument(
        "--experts",
        default="all",
        help="Comma-separated expert IDs for split inference, or 'all'. Use 'all' for real alignment.",
    )
    parser.add_argument("--save", default="", help="Optional .pt path to save raw comparison tensors.")
    parser.add_argument("--json-report", default="", help="Optional JSON path for structured comparison metrics.")
    parser.add_argument("--markdown-report", default="", help="Optional Markdown path for a human-readable report.")
    args = parser.parse_args()

    tokenizer = AutoTokenizer.from_pretrained(args.model)
    input_ids = _prepare_input(tokenizer, args.prompt, args.chat_template)
    print(f"input_ids={input_ids.tolist()}")
    print(f"input_len={input_ids.shape[1]}")

    expert_ids = None if args.experts == "all" else [int(item) for item in args.experts.split(",") if item]
    dtype = _resolve_dtype(args.dtype)
    original = run_original(args.model, input_ids, dtype, args.max_new_tokens, args.skip_generate)
    split = run_split(args.manifest, input_ids, dtype, args.max_new_tokens, expert_ids, args.skip_generate)
    print_comparison(original, split, tokenizer)

    if args.json_report or args.markdown_report:
        report = build_report(
            original,
            split,
            tokenizer,
            model_path=args.model,
            manifest_path=args.manifest,
            dtype_name=str(dtype).replace("torch.", ""),
            prompt=args.prompt,
            input_ids=input_ids,
            expert_ids=expert_ids,
            max_new_tokens=args.max_new_tokens,
            skip_generate=args.skip_generate,
            use_chat_template=args.chat_template,
        )
        if args.json_report:
            json_report_path = Path(args.json_report)
            json_report_path.parent.mkdir(parents=True, exist_ok=True)
            json_report_path.write_text(json.dumps(report, indent=2, ensure_ascii=False, default=_json_default), encoding="utf-8")
            print(f"\nwrote JSON report to {json_report_path}")
        if args.markdown_report:
            write_markdown_report(report, args.markdown_report)
            print(f"wrote Markdown report to {args.markdown_report}")

    if args.save:
        save_path = Path(args.save)
        save_path.parent.mkdir(parents=True, exist_ok=True)
        torch.save({"input_ids": input_ids, "original": original, "split": split}, save_path)
        print(f"\nsaved raw tensors to {save_path}")


if __name__ == "__main__":
    main()
