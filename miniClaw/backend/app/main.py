"""
MiniClaw Backend - FastAPI Proxy Service
"""
from fastapi import FastAPI, HTTPException
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel
from typing import Optional
import httpx
import logging

# 配置日志
logging.basicConfig(level=logging.INFO)
logger = logging.getLogger(__name__)

app = FastAPI(
    title="MiniClaw Proxy Service",
    description="Lightweight proxy service for private deployment",
    version="1.0.0",
)

# CORS配置
app.add_middleware(
    CORSMiddleware,
    allow_origins=["*"],  # 生产环境应限制为具体域名
    allow_credentials=True,
    allow_methods=["*"],
    allow_headers=["*"],
)

# 请求模型
class ProxyRequest(BaseModel):
    url: str
    method: str = "GET"
    headers: Optional[dict] = None
    body: Optional[dict] = None

class ProxyResponse(BaseModel):
    status_code: int
    data: dict
    headers: dict
    error: Optional[str] = None

# 健康检查
@app.get("/")
async def root():
    return {
        "status": "healthy",
        "service": "miniClaw-proxy",
        "version": "1.0.0"
    }

@app.get("/health")
async def health_check():
    return {"status": "ok"}

# 代理端点
@app.post("/proxy")
async def proxy_request(request: ProxyRequest):
    """通用代理端点"""
    try:
        logger.info(f"Proxy request: {request.method} {request.url}")
        
        # 创建HTTP客户端
        async with httpx.AsyncClient() as client:
            # 准备请求参数
            req_kwargs = {
                "headers": request.headers or {},
            }
            
            if request.body:
                req_kwargs["json"] = request.body
            
            # 发送请求 - 根据方法调用对应的函数
            method_upper = request.method.upper()
            
            if method_upper == "GET":
                response = await client.get(request.url, **req_kwargs)
            elif method_upper == "POST":
                response = await client.post(request.url, **req_kwargs)
            elif method_upper == "PUT":
                response = await client.put(request.url, **req_kwargs)
            elif method_upper == "DELETE":
                response = await client.delete(request.url, **req_kwargs)
            elif method_upper == "PATCH":
                response = await client.patch(request.url, **req_kwargs)
            elif method_upper == "HEAD":
                response = await client.head(request.url, **req_kwargs)
            elif method_upper == "OPTIONS":
                response = await client.options(request.url, **req_kwargs)
            else:
                # 默认使用GET
                response = await client.get(request.url, **req_kwargs)
            
            # 返回响应
            content_type = response.headers.get("content-type", "")
            if content_type.startswith("application/json"):
                try:
                    data = response.json()
                except:
                    data = response.text
            else:
                data = response.text
            
            return {
                "status_code": response.status_code,
                "data": data,
                "headers": dict(response.headers),
                "error": None
            }
            
    except httpx.HTTPError as e:
        logger.error(f"HTTP error: {str(e)}")
        raise HTTPException(status_code=500, detail=f"Proxy error: {str(e)}")
    except Exception as e:
        logger.error(f"Unexpected error: {str(e)}")
        raise HTTPException(status_code=500, detail=str(e))

# 配置端点
@app.get("/config")
async def get_config():
    """获取服务配置"""
    return {
        "service": "miniClaw",
        "version": "1.0.0",
        "mode": "proxy",
        "features": ["proxy", "cors", "health-check"],
        "proxy_enabled": True
    }

# 简单的GET代理（用于浏览器直接访问）
@app.get("/proxy/simple")
async def simple_proxy(url: str, method: str = "GET"):
    """简单的GET代理，可直接在浏览器中使用"""
    try:
        async with httpx.AsyncClient() as client:
            method_upper = method.upper()
            
            if method_upper == "GET":
                response = await client.get(url)
            elif method_upper == "POST":
                response = await client.post(url)
            elif method_upper == "PUT":
                response = await client.put(url)
            elif method_upper == "DELETE":
                response = await client.delete(url)
            else:
                response = await client.get(url)
            
            content_type = response.headers.get("content-type", "")
            if content_type.startswith("application/json"):
                try:
                    data = response.json()
                except:
                    data = response.text
            else:
                data = response.text
            
            return {
                "status_code": response.status_code,
                "data": data,
                "headers": dict(response.headers),
                "error": None
            }
    except Exception as e:
        raise HTTPException(status_code=500, detail=str(e))

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8000)