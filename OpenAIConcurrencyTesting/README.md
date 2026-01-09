# OpenAI Concurrency Testing

这个项目用于测试OpenAI 格式 API的并发性能和连接稳定性。

## 工具

### connection_test.py

用于测试OpenAI API连接的基本脚本。

你可以在这个脚本中首先验证你的API密钥和URL是否正确，然后运行压力测试。

#### 配置

脚本支持通过 `.env` 文件进行配置：

1. 复制 `.env.example` 为 `.env`：
   ```
   cp .env.example .env
   ```

2. 编辑 `.env` 文件，填写您的配置：
   ```
   MODEL_URL=http://your-model-server/v1
   MODEL_KEY=your-api-key-here
   MODEL_NAME=your-model-name
   ```

如果 `.env` 文件存在，脚本会自动加载其中的环境变量；否则使用默认的占位符值。

### llm-benchmark

我们使用 [lework/llm-benchmark](https://github.com/lework/llm-benchmark) 工具来进行LLM模型的压测。

#### 功能特点
- 多阶段并发测试（从低并发逐步提升到高并发）
- 自动化测试数据收集和分析
- 详细的性能指标统计和可视化报告
- 支持短文本和长文本测试场景
- 灵活的配置选项
- 生成 JSON 输出以便进一步分析或可视化

#### 使用方法
0. 克隆仓库：
   ```
   git clone https://github.com/lework/llm-benchmark.git
    ```

1. 进入 llm-benchmark 目录：
   ```
   cd llm-benchmark
   ```

2. 安装依赖：
   ```
   pip install -r requirements.txt
   ```

3. 运行全套性能测试：
   ```
   python run_benchmarks.py \
     --llm_url "http://your-llm-server" \
     --api_key "your-api-key" \
     --model "your-model-name" \
     --use_long_context
   ```

4. 运行单次并发测试：
   ```
   python llm_benchmark.py \
     --llm_url "http://your-llm-server" \
     --api_key "your-api-key" \
     --model "your-model-name" \
     --num_requests 100 \
     --concurrency 10
   ```

#### 测试报告

运行测试后，结果会保存在 `llm-benchmark/output/` 目录中。

您可以使用提供的 `convert_json_to_md.py` 脚本将 JSON 结果转换为 Markdown 格式：

```
python convert_json_to_md.py llm-benchmark/output/benchmark_results_YYYYMMDD_HHMMSS.json
```

这将生成一个对应的 `.md` 文件，便于查看和分享测试报告。

#### 指标说明：RPS、TPS 与首Token延迟（TTFT）

- **RPS（Requests Per Second）**：单位时间内系统处理的请求数，反映系统吞吐能力。RPS 主要受并发数、请求排队和模型处理速度影响；短输出、短请求通常能获得较高的 RPS。  

- **TPS（Tokens Per Second）**：单位时间内生成的 token 数（即总输出 token / 总用时），反映生成吞吐率。TPS 与输出长度密切相关——输出越长，TPS 对用户感知吞吐更有意义。  

- **首Token延迟（Time to First Token, TTFT）**：从发送请求到模型返回第一个 token 的时间。TTFT 反映模型的启动/排队延迟与首响应速度，对于交互体验尤为关键。  

诊断要点（并发增加时的变化含义）：
- 若 **RPS 上升**，说明系统在更高并发下仍能处理更多请求（吞吐提升）。  
- 若 **RPS 不升反降或持平**，说明系统可能进入饱和或出现资源争抢（CPU/GPU/网络/进程池），需要检查资源或降低单请求开销。  
- 若 **TTFT 随并发增加显著上升**，通常说明请求在模型端出现排队或冷启动/动态加载开销（模型被频繁唤醒或没有足够的工作线程）；这会导致首包响应慢，影响交互体验。  
- 若 **平均延迟远高于 TTFT** 则说明生成过程耗时（生成速率慢或输出长）；若 **TTFT 显著高于平均延迟** 则更可能是排队或调度延迟。  

调优建议：
- 对于 TTFT 上升问题，可尝试：增加模型工作线程/实例、调整并发限制或批处理设置、使用模型预热（warm-up）请求、确保网络和连接重用（keep-alive）。  
- 对于 RPS/TPS 瓶颈，可检查 GPU/CPU 利用率、内存/显存占用、以及是否存在频繁的模型加载或过小/过大的 batch 设置；必要时增加实例或使用更适合高并发的部署方式（模型服务池、推理引擎）。  

#### 细粒度并发测试（示例：并发数 1~30）

如果您希望对 1 到 30 的并发点做更细粒度的压测，有两种常用做法：

1) 手动修改 `run_benchmarks_myself.py`（最简单、直接）

- 将 `run_benchmarks.py` 复制为 `run_benchmarks_myself.py`，然后修改其中的并发配置部分为：
```python
configurations = [
    {"num_requests": 100, "concurrency": c, "output_tokens": 100}
    for c in range(1, 31)
]
```

- 说明：
  - `num_requests` 建议设为 100 或更多以保证统计稳定性；
  - `output_tokens` 根据需要调整生成长度；
  - 可在两次跑点之间调整 `time.sleep(...)` 的冷却时间（建议 3~10 秒）。

1) 使用命令行循环（PowerShell 示例）

如果您不想改脚本，可以使用循环依次调用单次测试：

```powershell
for ($c=1; $c -le 30; $c++) {
  python llm_benchmark.py --llm_url "http://your-llm-server" --api_key "" --model "my-model" --num_requests 100 --concurrency $c --output_tokens 100 --request_timeout 120 --output_format json >> llm-benchmark/output/results_raw.json
  Start-Sleep -s 5
}
```

- 说明：此方法会将每次的 JSON 输出追加到 `results_raw.json`，需在后续做合并/清洗以生成标准数组格式供 `convert_json_to_md.py` 使用。

建议：为保证结果稳定，尽量顺序运行（避免同时对系统施加多个并发实验），并对关键并发点（如 10、20、30）做多轮重复以取平均。