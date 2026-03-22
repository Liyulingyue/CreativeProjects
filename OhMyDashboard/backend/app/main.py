from fastapi import FastAPI
from fastapi.middleware.cors import CORSMiddleware
from .services import SystemService
from datetime import datetime

app = FastAPI(title="OhMyDashboard API")

# 配置 CORS，允许前端开发环境下跨域访问
app.add_middleware(
    CORSMiddleware,
    allow_origins=["*"],
    allow_credentials=True,
    allow_methods=["*"],
    allow_headers=["*"],
)

@app.get("/api/system/info")
async def get_system_info():
    """获取基础系统 CPU、内存和磁盘信息"""
    return SystemService.get_system_info()

@app.get("/api/system/docker")
async def get_docker_info():
    """获取宿主机 Docker 容器状态"""
    return SystemService.get_docker_containers()

@app.get("/api/system/processes")
async def get_process_list(limit: int = 50):
    """获取当前活跃进程列表"""
    return SystemService.get_running_processes(limit=limit)

@app.get("/api/system/startup")
async def get_startup_stats():
    """获取负载和开机/运行相关的状态统计"""
    return SystemService.get_startup_info()

@app.get("/api/status")
async def get_status():
    """通用状态检测路由"""
    return {
        "status": "online",
        "time": datetime.now().isoformat()
    }

@app.get("/")
async def root():
    return {"message": "OhMyDashboard Backend is Live! 🚀 (v2)"}
