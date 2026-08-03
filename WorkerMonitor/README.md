# WorkerMonitor

健康监控桌面应用，带摄像头姿态检测和休息提醒。

## 项目结构

```
WorkerMonitor/
├── backend/          # Rust HTTP API 服务器
│   └── resource/    # 放置 end2end.onnx 模型文件
├── frontend/         # React UI
├── src-tauri/       # Tauri 桌面壳
└── README.md
```

## 快速开始

### 1. 安装依赖

```bash
# 安装前端依赖
cd frontend && npm install && cd ..

# 安装 Rust 依赖
cd backend && cargo build --release && cd ..
```

### 2. 获取模型文件

模型文件不在仓库中，首次编译前需要下载：

**自动下载（推荐）**：
```powershell
# 在 backend 目录下运行
.\scripts\download-model.ps1
```

**手动下载**：
1. 下载地址：https://download.openmmlab.com/mmpose/v1/projects/rtmposev1/onnx_sdk/rtmpose-t_simcc-body7_pt-body7_420e-256x192-026a1439_20230504.zip
2. 解压后找到 `end2end.onnx`，复制到 `backend/resource/end2end.onnx`

### 3. 运行

**开发模式**（需要同时启动 backend 和 frontend）：

终端 1 - 启动后端：
```bash
cd backend
cargo run
```

终端 2 - 启动前端：
```bash
cd frontend
npm run dev
```

**生产模式**：
```bash
npm run build
```

## 模型说明

使用 RTMPose-t 模型进行人体姿态检测，输入 `[1x3x256x192]`，输出 COCO 17 点关键点。

模型来源：OpenMMLab MMDeploy RTMPose ONNX SDK

## 技术栈

- **前端**: React + Vite + TypeScript
- **后端**: Rust + Actix-web + ONNX Runtime
- **桌面**: Tauri 2
- **姿态检测**: RTMPose ONNX
