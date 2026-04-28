# QuickStart

## 环境准备

```bash
python -m venv .venv
source .venv/bin/activate
pip install -r requirements.txt
pip install -r backend/requirements.txt
```

## 1. Inquisitor（扫描模型）

分析模型结构，生成权重分布报告。不加载任何模型权重，纯元数据解析，毫秒级完成。

```bash
.venv/bin/python src/inquisitor_cli.py models/PaddlePaddle/ERNIE-4.5-21B-A3B-PT
```

**ERNIE-4.5-21B-A3B-PT 扫描结果：**

```
MoE Configuration:
  Experts per layer     : 64
  Activated Experts (K) : 6        ← 每个 token 激活 6 个
  Shared Experts        : 2        ← 每个 token 必过

  Backbone        size=1.73 GB   (4.3%)   ← 保留在 Master
  Gate/Router     size=346 MB     (0.8%)   ← 保留在 Master
  Expert          size=37.97 GB   (93.4%)  ← 可分发到 Worker
  Shared Expert   size=608 MB     (1.5%)   ← 保留在 Master
```

## 2. Surgeon（拆分模型）

将模型拆分为 backbone + expert 文件，生成 manifest 供后续分发。

```bash
# 完整拆分（所有 64 experts/layer）
.venv/bin/python src/surgeon_cli.py models/PaddlePaddle/ERNIE-4.5-21B-A3B-PT \
    --output output/splits/ERNIE-4.5-21B-A3B-PT-full

# Mini-mode：只提取前 6 个 expert/layer（约 3.6 GB，用于本地验证）
.venv/bin/python src/surgeon_cli.py models/PaddlePaddle/ERNIE-4.5-21B-A3B-PT \
    --fixed-k 6 \
    --output output/splits/ERNIE-4.5-21B-A3B-PT-k6
```

**输出结构：**

```
output/splits/<model_name>/
├── manifest.json           ← 元数据清单
├── backbone/
│   ├── backbone.safetensors      (骨干网络，185 weights)
│   ├── gate.safetensors         (路由 gate)
│   └── shared_expert.safetensors (共享专家)
└── experts/
    ├── expert_000_layer_001.safetensors
    ... 1728 个 expert 文件 ...
    └── expert_063_layer_027.safetensors
```

## 3. Mini-mode（本地验证推理）

不依赖 Worker，在 Master 本地运行固定路由推理。（需先运行 `surgeon_cli.py --fixed-k 6` 生成 k6 拆分）

```bash
.venv/bin/python src/inference_chat.py \
    --manifest output/splits/ERNIE-4.5-21B-A3B-PT-k6/manifest.json \
    --experts 0,1,2,3,4,5 \
    --prompt "今天天气真好，" \
    --max-tokens 20
```

## 4. Nexus（分布式推理）

### 4.1 启动 Master

```bash
.venv/bin/python src/nexus/master.py \
    --manifest output/splits/ERNIE-4.5-21B-A3B-PT-full/manifest.json \
    --port 5000 \
    --host 127.0.0.1 \
    --dtype fp16 \
    --experts 0,1,2,3,4,5 \
    --log-file logs/master.log \
    --kv-cache
```

`--kv-cache` 启用 KV 缓存加速（实验性功能，后续完善）。

`--log-file` 将日志同时输出到文件，方便排查 TCP 分发、Worker 注册等分布式问题。Master 启动后本地加载 6 个 experts (0~5)，同时监听 Worker 注册。`--dtype` 必须与 Worker 保持一致。

### 4.2 启动 Worker

```bash
.venv/bin/python src/nexus/worker.py \
    --id worker-1 \
    --http-port 8000 \
    --tcp-port 9000 \
    --host 127.0.0.1 \
    --dtype float32 \
    --experts-dir output/splits/ERNIE-4.5-21B-A3B-PT-full/experts \
    --expert-ids 4,5,6 \
    --master http://localhost:5000/workers \
    --log-file logs/worker-1.log
```

Worker 启动后自动加载 experts (6~11)，并自动注册到 Master。Master 和 Worker 的 `--experts`/`--expert-ids` 不要重叠。

> **跨机器部署**：Worker 需要加 `--advertise-host <本机实际IP>` 参数，让 Master TCP 分发时能正确连接到 Worker。

### 4.3 Worker 注册 & 管理

```bash
# 查询 Worker 状态（Master 会实时查询 Worker 获取）
curl http://localhost:5000/workers/worker-1/status

# 查询 Master 状态
curl http://localhost:5000/status

# 列出所有 Worker
curl http://localhost:5000/workers
```

### 4.4 触发推理

```bash
curl -X POST http://localhost:5000/inference \
    -H "Content-Type: application/json" \
    -d '{"prompt":"今天天气真好，","max_tokens":20}'
```

### Master HTTP API

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/status` | Master 状态（含本地 expert 列表） |
| POST | `/workers` | Worker 注册 |
| GET | `/workers` | 列出所有已注册 Worker |
| GET | `/workers/{id}/status` | 查询指定 Worker 状态 |
| POST | `/workers/{id}/load` | 让指定 Worker 加载一个 expert |
| POST | `/workers/{id}/unload` | 让指定 Worker 卸载一个 expert |
| POST | `/load_expert` | 动态加载 expert 到 Master 本地 |
| POST | `/unload_expert` | 动态卸载 Master 本地 expert |
| POST | `/load_expert_to_worker` | 让 Master 命令 Worker 加载 expert |
| POST | `/unload_expert_from_worker` | 让 Master 命令 Worker 卸载 expert |
| POST | `/inference` | 触发推理 |

### Worker HTTP API

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/status` | Worker 状态（已加载的 experts） |
| POST | `/load` | 加载 expert 权重文件 |
| POST | `/unload` | 卸载 expert 释放内存 |
| POST | `/batch_load` | 批量加载多个 expert |
| POST | `/register` | 向 Master 注册（或更新信息） |

## 项目模块

| 模块 | 路径 | 状态 |
|------|------|------|
| Inquisitor | `src/inquisitor/` | ✅ 完成 |
| Surgeon | `src/surgeon/` | ✅ 完成 |
| inference_chat | `src/inference_chat.py` | ✅ 完成（mini-mode） |
| Nexus Master | `src/nexus/master.py` | ✅ 完成 |
| Nexus Worker | `src/nexus/worker.py` | ✅ 完成 |
| Backend | `backend/` | ✅ 完成 |
| Frontend | `frontend/` | ✅ 完成 |

## 5. Web UI（可选）

### 启动后端 API

```bash
.venv/bin/python backend/run.py
```

### 启动前端开发服务器

```bash
cd frontend && npm run dev
```

访问 http://localhost:5173，获得完整的开发体验（含热更新）。

### 构建生产版本

```bash
cd frontend && npm run build
# 构建输出至 frontend/dist/
```

## 6. 项目结构
