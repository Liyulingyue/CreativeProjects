# MiniClaw - 轻量级代理服务

## 🎯 项目概述

MiniClaw 是一个轻量级的代理服务，专为私有化部署设计。它提供简单的代理能力，无需集成飞书等第三方服务。

## 📁 项目结构

```
miniClaw/
├── backend/              # Python FastAPI 后端
│   ├── app/             # 源代码目录
│   │   ├── __init__.py
│   │   └── main.py      # FastAPI 应用主文件
│   ├── run.py           # 应用入口文件
│   └── requirements.txt  # Python 依赖
├── frontend/            # React + TypeScript 前端
│   ├── public/
│   ├── src/
│   ├── package.json
│   └── tsconfig.json
└── README.md            # 项目说明
```

## 🔧 后端 (FastAPI)

### 技术栈
- Python 3.x
- FastAPI
- Uvicorn (ASGI 服务器)
- Pydantic (数据验证)

### 快速开始

```bash
cd backend
python3 -m venv venv
source venv/bin/activate
pip install -r requirements.txt
python3 run.py
```

服务将在 `http://localhost:8000` 启动

### API 端点

- `GET /` - 根端点，健康检查
- `GET /health` - 健康检查
- `POST /proxy` - 代理请求
- `GET /config` - 获取配置

## 🎨 前端 (React + TypeScript)

### 技术栈
- React 18
- TypeScript
- Create React App

### 快速开始

```bash
cd frontend
npm install
npm start
```

开发服务器将在 `http://localhost:3000` 启动

## 🚀 部署

### 后端部署

```bash
cd backend
pip install -r requirements.txt
uvicorn app.main:app --host 0.0.0.0 --port 8000
```

### 前端部署

```bash
cd frontend
npm run build
# 将 build/ 目录部署到静态文件服务器
```

## 📝 特性

✅ 轻量级代理服务  
✅ FastAPI 高性能后端  
✅ React + TypeScript 前端  
✅ 完全私有化部署  
✅ 无需第三方服务集成  
✅ CORS 支持  
✅ 健康检查端点  

## 🔜 待开发功能

- [ ] 完整的代理逻辑实现
- [ ] 认证和授权
- [ ] 请求/响应日志
- [ ] 速率限制
- [ ] 配置管理

## 📄 许可证

MIT License - 详见 LICENSE 文件

## 👥 开发者

ClawBot (笠雨聆月的助手)
创建日期: 2026-02-18
版本: 1.0.0
状态: ✅ 基础架构已完成