# RaspbotController

个人封装的亚博智能的小车交互控制台，方便更好地控制小车的运动和功能。

## 项目结构

```
RaspbotController/
├── backend/                     # Python 后端服务
│   ├── app/                     # FastAPI 应用
│   │   ├── __init__.py         # 应用工厂
│   │   ├── hardware/            # 硬件控制层（subprocess 调用）
│   │   │   ├── car_control.py   # I2C 电机/舵机控制 (smbus2)
│   │   │   └── camera_control.py # 摄像头控制 (Picamera2)
│   │   └── robot/               # 机器人控制类
│   │       └── car.py           # YB_Pcb_Car 封装
│   ├── requirements.txt         # Python 依赖
│   ├── run.py                   # 启动脚本: uvicorn app:get_app() --host 0.0.0.0 --port 8000
│   └── start.sh                 # 完整启动脚本（自动创建 .venv 并杀死冲突进程）
├── frontend/                    # React 前端
│   └── src/
│       ├── api/                 # API 调用（使用 Vite 代理）
│       ├── components/          # UI 组件（RobotController, CameraView, 等）
│       └── hooks/               # React hooks
├── .gitignore
├── AGENT_INSTRUCTION.md
└── README.md
```

## 快速开始

### 后端（API 服务器）

```bash
# 从 backend 目录启动
cd backend
./start.sh
# 或手动：
# . .venv/bin/activate
# python run.py
```

API 将在 `http://<树莓派IP>:8000` 上提供
文档: `http://<树莓派IP>:8000/docs`

### 前端（开发模式）

```bash
# 从 frontend 目录启动
cd frontend
npm install  # 第一次只需运行一次
npm run dev
```

前端将在 `http://<树莓派IP>:5173` 上提供
- 前端使用 Vite 开发服务器
- 所有 `/api/v1/*` 请求会被代理到后端 `http://localhost:8000/*`
- 从其他设备访问时，使用树莓派的实际 IP 地址替换 `localhost`

### 生产构建

```bash
cd frontend
npm run build
# 构建输出在 frontend/dist/
# 可以由任何静态文件服务器托管
```

## API 接口

所有端点均相对于 `http://<树莓派IP>:8000`：

| 方法 | 路径 | 描述 |
|------|------|------|
| GET | `/health` | 健康检查 |
| POST | `/api/v1/car/move` | 移动控制 (forward/backward/left/right/spin_left/spin_right/stop) |
| POST | `/api/v1/car/stop` | 停止 |
| POST | `/api/v1/car/manual` | 手动控制左右电机速度 |
| POST | `/api/v1/servo/control` | 云台舵机控制 (id + angle) |
| GET | `/api/v1/camera/snapshot` | 摄像头快照 (JPEG) |

注意：前端代理将 `/api/v1/*` 重写为 `/ *` 发送到后端。

## 键盘控制

点击控制面板后，使用：
- W / ↑ : 前进
- S / ↓ : 后退
- A / ← : 左转
- D / → : 右转
- 空格 : 停止

## 技术栈

- **后端**: FastAPI + uvicorn + smbus2
- **前端**: React + TypeScript + Vite
- **硬件**: Raspberry Pi + Picamera2 + I2C 控制
