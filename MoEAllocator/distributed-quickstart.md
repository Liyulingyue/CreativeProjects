# 分布式 MoE 快速验证指南（无前后端）

## 架构

| 机器 | 角色 | 职责 |
|------|------|------|
| Machine 1 | Master | 加载 backbone/gate/shared expert + 前 32 个 expert 列 |
| Machine 2 | Worker | 加载后 32 个 expert 列，通过 TCP 分发 |

---

## 前提

- 两台机器网络互通
- 共享模型文件（建议通过 NFS 或 rsync 同步 `output/splits/ERNIE-4.5-21B-A3B-PT-full/`）
- 端口开放：`5000`(master HTTP), `9000`(master TCP), `8000`(worker HTTP), `11000`(worker TCP)

---

## 启动顺序

### Machine 1 — Master

```bash
cd MoEAllocator

python -m src.nexus.master \
    --manifest output/splits/ERNIE-4.5-21B-A3B-PT-full/manifest.json \
    --port 5000 \
    --host 0.0.0.0 \
    --dtype fp16 \
    --experts 0,1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,19,20,21,22,23,24,25,26,27,28,29,30,31,32,33,34,35,36,37,38,39,40,41,42,43,44,45,46,47,48,49 \
    --log-file logs/master.log
```

解释：Master 在本地加载 expert 0~49（前 50 列 × 27 层 = 1350 个文件），使用 FP16 精度。Master 和 Worker 的 expert 范围不要重叠。

---

### Machine 2 — Worker

```bash
cd MoEAllocator

python -m src.nexus.worker \
    --id worker-1 \
    --http-port 8000 \
    --tcp-port 11000 \
    --host 0.0.0.0 \
    --advertise-host <MACHINE2_IP> \
    --dtype fp16 \
    --experts-dir output/splits/ERNIE-4.5-21B-A3B-PT-full/experts \
    --expert-ids 50,51,52,53,54,55,56,57,58,59,60,61,62,63 \
    --master http://<MACHINE1_IP>:5000/workers \
    --log-file logs/worker-1.log
```

解释：Worker 绑定 `0.0.0.0`，但通过 `--advertise-host` 告诉 Master 自己的实际 IP；Master TCP 分发时使用该地址连接 Worker。Worker 在本地加载 expert 50~63（后 14 列 × 27 层 = 378 个文件），启动后自动注册到 Machine 1 的 Master。

---

## 验证流程

### 1. 确认 Worker 已注册

在 **Machine 1** 终端查看 Master 日志，应看到：

```
Worker registered: worker-1 at <MACHINE2_IP>:8000/11000 (advertise=<MACHINE2_IP>), experts=864
```

Worker 加载 864 个文件（32 列 × 27 层）。

### 2. 确认 Routing 表正确

Master 日志中，每次推理时 `_build_routing_table()` 会将所有 Worker 的 expert 注册信息构建为映射：

```
{(layer, expert) -> WorkerInfo}
```

例如 `(1, 32)` → worker-1，`(1, 0)` → 本地。

### 3. 执行推理（任一机器）

```bash
curl -X POST http://<MACHINE1_IP>:5000/inference \
    -H "Content-Type: application/json" \
    -d '{"prompt": "今天天气真好，", "max_tokens": 10}'
```

Gate 会为每层选出 top-6 expert：
- 如果选中的 expert 在 0~31 范围 → Master 本地执行
- 如果选中的 expert 在 32~63 范围 → TCP 分发给 Machine 2 的 Worker

### 4. Master 日志观察

```
Layer 1: token0 selected=[15, 3, 42, 8, 27, 55]
  -> dispatch token-r0 expert_42_layer_1 to worker-1
  -> dispatch token-r0 expert_55_layer_1 to worker-1
  Layer 1: local=4, dispatched=2, fallback=0, total=6
```

说明 expert 42、55 不在 Master 本地，TCP 分发给了 Worker。

---

## 关键设计

- **Worker 可访问地址**：Worker 用 `--host` 绑定监听地址，用 `--advertise-host` 告知 Master 自己的对外地址。跨机器时 `--advertise-host` 必须设为实际 IP，否则 Master TCP 分发会连接 `0.0.0.0` 而失败
- **Routing 按 (layer, expert) 独立**：Worker 32~63 列的 expert 只在 Machine 2 上，Master 分发时会查 routing 表找到对应 Worker
- **Worker 动态 load/unload**：Worker 可随时动态加载/卸载 expert，load/unload 后自动重新注册到 Master 更新 routing
- **Master 本地 load**：Master 也可以通过 HTTP API 动态加载 expert 到本地（`curl -X POST http://<MACHINE1_IP>:5000/load_expert -d '{"expert_id":5,"layer_id":1}'`）

## 精度控制

```bash
--dtype fp16    # float16，省内存（约减半）
--dtype bf16    # bfloat16，动态范围同 float32，精度同 fp16
--dtype float32 # 默认，完整精度
```

**重要**：Master 和 Worker 必须使用相同 `--dtype`，否则 TCP 分发时数据类型不匹配会导致结果错误。
