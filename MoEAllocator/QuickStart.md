# QuickStart

## 环境准备

```bash
python -m venv .venv
source .venv/bin/activate
pip install -r requirements.txt
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
    --output output/splits/ERNIE-4.5-21B-A3B-PT

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

不依赖 Worker，在 Master 本地运行固定路由推理。

```bash
.venv/bin/python src/inference_chat.py \
    --manifest output/splits/ERNIE-4.5-21B-A3B-PT-k6/manifest.json \
    --experts "0,1,2,3,4,5" \
    --prompt "今天天气真好，" \
    --max-tokens 20
```

## 4. Nexus（分布式推理）

### 4.1 启动 Master

```bash
.venv/bin/python src/nexus/master.py \
    --manifest output/splits/ERNIE-4.5-21B-A3B-PT-k6/manifest.json \
    --port 5000 \
    --experts "0,1,2"
```

Master 启动后，本地加载 3 个 experts (0,1,2)，同时监听 Worker 注册。

### 4.2 启动 Worker

```bash
.venv/bin/python src/nexus/worker.py \
    --id worker-1 \
    --http-port 8000 \
    --tcp-port 9000 \
    --experts-dir output/splits/ERNIE-4.5-21B-A3B-PT-k6/experts \
    --expert-ids "3,4,5" \
    --master http://localhost:5000/workers
```

Worker 启动后自动加载 experts (3,4,5)，并自动注册到 Master。

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
| GET | `/status` | Master 状态 |
| POST | `/workers` | 注册 Worker |
| GET | `/workers` | 列出所有 Worker |
| GET | `/workers/{id}/status` | Worker 状态 |
| POST | `/workers/{id}/load` | 让 Worker 加载 expert |
| POST | `/workers/{id}/unload` | 让 Worker 卸载 expert |
| POST | `/inference` | 触发推理 |

### Worker HTTP API

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/status` | Worker 状态（已加载的 experts） |
| POST | `/load` | 加载 expert 权重文件 |
| POST | `/unload` | 卸载 expert 释放内存 |
| POST | `/register` | 向 Master 注册 |

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

启动后端 API 服务：

```bash
.venv/bin/python -m uvicorn backend.main:app --host 0.0.0.0 --port 8000
```

启动 React 前端：

```bash
cd frontend && npm run dev
```

访问 http://localhost:5173，可视化管理 Master/Worker 节点、动态加载/卸载 Experts、查看状态、执行推理。
