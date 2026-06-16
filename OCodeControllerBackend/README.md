# OpenCodeHarmonyBackend
> 当前转移到 https://github.com/Liyulingyue/OCodeController 继续开发

基于 FastAPI 的后端服务，为 OpenCodeHarmony App 提供配置和会话同步功能。

## 功能

- 用户认证 (JWT)
- 后端配置同步
- 会话元数据同步
- 设备管理

## 快速开始

```bash
cd OpenCodeHarmonyBackend
pip install -r requirements.txt
uvicorn app.main:app --reload
```

## API 文档

启动后访问 http://localhost:8000/docs
