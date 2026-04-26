# MoEAllocator 后端

FastAPI 后端服务，为 MoEAllocator Core 提供进程管理、HTTP 转发和 REST API。

## 启动

```bash
cd MoEAllocator
.venv/bin/python -m uvicorn backend.main:app --host 0.0.0.0 --port 8000
```

API 文档: http://localhost:8000/docs

## API 接口

### 节点管理

| 方法 | 路径 | 描述 |
|------|------|------|
| GET | `/api/nodes` | 列出所有节点 |
| POST | `/api/nodes/master` | 创建 Master 节点 |
| POST | `/api/nodes/worker` | 创建 Worker 节点 |
| GET | `/api/nodes/{node_id}` | 获取节点信息 |
| DELETE | `/api/nodes/{node_id}` | 删除节点 |
| GET | `/api/nodes/{node_id}/status` | 获取节点状态 |
| GET | `/api/nodes/{node_id}/logs` | 获取节点日志 |
| GET | `/api/nodes/{node_id}/workers` | 获取 Master 的 Workers 列表 |
| POST | `/api/nodes/{master_id}/workers` | 手动注册 Worker 到 Master |

### Expert 动态管理

| 方法 | 路径 | 描述 |
|------|------|------|
| POST | `/api/nodes/master/load_expert` | 在 Master 运行时加载 Expert |
| POST | `/api/nodes/master/unload_expert` | 在 Master 运行时卸载 Expert |

### 推理

| 方法 | 路径 | 描述 |
|------|------|------|
| POST | `/api/inference` | 执行推理 |

## 架构

```
Frontend (React, :5173)
    ↓ HTTP /api/*
Backend (FastAPI, :8000) — 进程管理 + HTTP 转发
    ↓ subprocess / HTTP
Core: Master + Worker — 真正推理
```

## 前端

React 前端位于 `frontend/` 目录。启动开发服务器：

```bash
cd frontend && npm run dev
```

前端功能：
- **节点管理**：创建/删除 Master 和 Worker 节点
- **Expert 管理**：在 Master 运行时动态加载/卸载 Experts
- **状态监控**：查看节点实时状态和已加载的 Experts 列表
- **推理**：输入 prompt 执行分布式推理
- **日志**：查看节点运行日志
