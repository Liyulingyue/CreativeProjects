#!/usr/bin/env python3
"""
将 llm-benchmark 的 JSON 结果转换为 Markdown 格式的脚本
"""

import json
import sys
from pathlib import Path

def convert_json_to_md(json_file_path):
    """将 JSON 文件转换为 Markdown 格式（生成汇总表 + 详细指标）"""

    # 读取 JSON 文件
    with open(json_file_path, 'r', encoding='utf-8') as f:
        data = json.load(f)

    # 汇总表头
    md = ["# 测试结果汇总\n"]

    # 计算聚合信息
    total_output_tokens = sum(r.get('total_output_tokens', 0) for r in data)
    total_test_time = sum(r.get('total_time', 0.0) for r in data)
    avg_tokens_per_sec = total_output_tokens / total_test_time if total_test_time > 0 else 0.0
    model_name = data[0].get('model') if data else 'N/A'
    use_long = any(r.get('use_long_context', False) for r in data)

    # 基本信息块
    md.append("## 基本信息")
    md.append("| 属性 | 值 |")
    md.append("|---|---:|")
    md.append(f"| 模型 | {model_name} |")
    md.append(f"| 长文本模式 | {'是' if use_long else '否'} |")
    md.append(f"| 总生成Token数 | {total_output_tokens:,} |")
    md.append(f"| 总测试时间 | {total_test_time:.2f} 秒 |")
    md.append(f"| 平均Token生成速率 | {avg_tokens_per_sec:.2f} tokens/sec |")
    md.append("")

    md.append("## 详细性能指标")
    md.append("| 并发数 | RPS | 平均延迟(秒) | P99延迟(秒) | 平均TPS | 首Token延迟(秒) | 成功率 |")
    md.append("|--------:|------:|-------------:|------------:|--------:|----------------:|-------:|")

    # 用于统计最佳配置
    best_rps = (0, None)  # (rps, concurrency)
    min_latency = (float('inf'), None)  # (avg_latency, concurrency)

    # 表格行与详细块
    details = []
    for result in data:
        conc = result.get('concurrency')
        rps = result.get('requests_per_second', 0.0)
        avg_lat = result.get('latency', {}).get('average', 0.0)
        p99 = result.get('latency', {}).get('p99', 0.0)
        avg_tps = result.get('tokens_per_second', {}).get('average', 0.0)
        first_token = result.get('time_to_first_token', {}).get('average', 0.0)
        success_rate = 0.0
        total = result.get('total_requests', 0)
        succ = result.get('successful_requests', 0)
        if total:
            success_rate = succ / total * 100.0

        # 更新最佳值
        if rps > best_rps[0]:
            best_rps = (rps, conc)
        if avg_lat < min_latency[0]:
            min_latency = (avg_lat, conc)

        md.append(f"| {conc} | {rps:.2f} | {avg_lat:.3f} | {p99:.3f} | {avg_tps:.2f} | {first_token:.3f} | {success_rate:.1f}% |")

        # 生成详细块，稍后输出
        block = []
        block.append(f"### 并发 {conc} 的详细指标")
        block.append(f"- **模型**: {result.get('model')}  ")
        block.append(f"- **总请求数**: {total}  ")
        block.append(f"- **成功请求数**: {succ}  ")
        block.append(f"- **总用时**: {result.get('total_time', 0.0):.2f}s  ")
        block.append(f"- **Requests/sec**: {rps:.2f}  ")
        block.append("\n**Latency (秒)**")
        lat = result.get('latency', {})
        block.append(f"- 平均: {lat.get('average', 0.0):.4f}  ")
        block.append(f"- P50: {lat.get('p50', 0.0):.4f}  ")
        block.append(f"- P95: {lat.get('p95', 0.0):.4f}  ")
        block.append(f"- P99: {lat.get('p99', 0.0):.4f}  ")
        block.append("\n**Tokens/sec**")
        tps = result.get('tokens_per_second', {})
        block.append(f"- 平均: {tps.get('average', 0.0):.4f}  ")
        block.append("\n**首Token延迟 (秒)**")
        tt = result.get('time_to_first_token', {})
        block.append(f"- 平均: {tt.get('average', 0.0):.4f}  ")

        details.append('\n'.join(block))

    # 最佳配置小结
    md.append("")
    md.append("## 性能最佳配置")
    if best_rps[1] is not None:
        md.append(f"- **最高 RPS**: 并发数 {best_rps[1]} ({best_rps[0]:.2f} req/sec)")
    if min_latency[1] is not None:
        md.append(f"- **最低平均延迟**: 并发数 {min_latency[1]} ({min_latency[0]:.3f} 秒)")

    md.append("")
    md.append("## 性能建议")
    md.append("- 系统若未达到性能瓶颈，可尝试增加更高的并发数以观察吞吐变化。")
    md.append("- 若首Token延迟显著高于平均延迟，建议排查模型启动或排队延迟。")

    md.append("\n---\n")

    # 追加详细内容
    md.append("## 详细测试指标")
    md.extend(details)

    return "\n".join(md)

def main():
    if len(sys.argv) != 2:
        print("Usage: python convert_json_to_md.py <json_file>")
        print("Example: python convert_json_to_md.py benchmark_results_20260109_113102.json")
        sys.exit(1)

    json_file = sys.argv[1]
    json_path = Path(json_file)

    if not json_path.exists():
        print(f"Error: File '{json_file}' does not exist.")
        sys.exit(1)

    # 生成 Markdown 文件名
    md_file = json_path.stem + ".md"
    md_path = json_path.parent / md_file

    # 转换并写入
    md_content = convert_json_to_md(json_path)
    with open(md_path, 'w', encoding='utf-8') as f:
        f.write(md_content)

    print(f"Markdown file generated: {md_path}")

if __name__ == "__main__":
    main()