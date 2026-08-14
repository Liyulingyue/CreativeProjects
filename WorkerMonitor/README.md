# WorkerMonitor

一个基于 Rust + React 的桌面健康监控应用，包含摄像头姿态识别与久坐提醒。

## 单栈维护声明（重要）

当前项目**只维护一套运行栈**：`wry + tao`。

- 唯一桌面入口：`src/main.rs`
- 唯一前端入口：`frontend/src`

如果你使用 `cargo run` 或 `npm run dev:desktop`，实际运行的是 `src/main.rs` 这套实现。

## 当前真实结构

```
WorkerMonitor/
├── Cargo.toml            # Rust 桌面主程序（wry + tao）
├── src/                  # Rust 核心逻辑（监控、摄像头、姿态检测）
├── frontend/             # React + Vite 前端
│   ├── src/
│   └── package.json
├── icons/
└── README.md
```

## 编辑边界

- 修改托盘、窗口大小、紧凑模式、IPC：编辑 `src/main.rs`
- 修改页面 UI：编辑 `frontend/src`

## 环境要求

- Node.js 18+
- Rust stable toolchain
- Windows 下建议安装 Visual Studio Build Tools（C++ 工具链）

## 快速开始

### 1. 安装前端依赖

```bash
cd frontend
npm install
cd ..
```

### 2. 开发调试

启动前端开发服务器：

```bash
npm run dev:web
```

另开一个终端启动桌面端：

```bash
npm run dev:desktop
```

说明：桌面端在 `DEV_MODE=1` 时默认连接 `http://localhost:5175`（与当前 Vite 配置一致）。
默认不写入文件日志。排查问题时可先设置 `WORKER_MONITOR_ENABLE_LOG=1`，日志会写入 `%LOCALAPPDATA%\WorkerMonitor\logs\worker-monitor.log.YYYY-MM-DD`。

### 3. 构建

```bash
npm run build
```

该命令会先构建前端，再执行 Rust release 构建。

如果出现 `failed to remove ... worker-monitor.exe` 或 `拒绝访问 (os error 5)`，通常是旧版程序仍在运行（含托盘中）。先退出正在运行的 WorkerMonitor，再重新执行构建。

## 常用命令

- `npm run dev:web`：启动前端开发服务器
- `npm run dev:desktop`：启动 Rust 桌面程序
- `npm run build:web`：仅构建前端
- `npm run build`：前端 + Rust release 构建
- `npm run check`：Rust 检查

## 技术栈

- 前端：React + Vite + TypeScript
- 桌面：Rust + wry + tao
- 摄像头：nokhwa
