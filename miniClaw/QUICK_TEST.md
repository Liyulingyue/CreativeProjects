# MiniClaw 快速测试指南

## ✅ 代理功能测试

### 1. 后端服务状态
```bash
# 检查健康状态
curl http://localhost:8000/
# 输出: {"status":"healthy","service":"miniClaw-proxy","version":"1.0.0"}

# 检查健康检查
curl http://localhost:8000/health
# 输出: {"status":"ok"}
```

### 2. 代理接口测试

#### POST 代理（推荐）
```bash
curl -X POST http://localhost:8000/proxy \
  -H "Content-Type: application/json" \
  -d '{"url":"https://jsonplaceholder.typicode.com/posts/1","method":"GET"}'
```

#### GET 简单代理
```bash
curl "http://localhost:8000/proxy/simple?url=https://jsonplaceholder.typicode.com/posts/1&method=GET"
```

### 3. 前端测试

访问 http://localhost:3000 并测试代理功能：
- 输入测试 URL: `https://jsonplaceholder.typicode.com/posts/1`
- 选择方法: GET
- 点击 "Send Proxy Request"
- 查看响应结果

## 🧪 完整测试脚本

```bash
#!/bin/bash
echo "=== MiniClaw 代理功能测试 ==="
echo ""

echo "1. 测试后端健康状态..."
curl -s http://localhost:8000/ | python3 -m json.tool
echo ""

echo "2. 测试代理接口 (POST)..."
curl -s -X POST http://localhost:8000/proxy \
  -H "Content-Type: application/json" \
  -d '{"url":"https://jsonplaceholder.typicode.com/posts/1","method":"GET"}' \
  | python3 -m json.tool
echo ""

echo "3. 测试简单代理 (GET)..."
curl -s "http://localhost:8000/proxy/simple?url=https://jsonplaceholder.typicode.com/posts/1&method=GET" \
  | python3 -m json.tool
echo ""

echo "4. 测试配置接口..."
curl -s http://localhost:8000/config | python3 -m json.tool
echo ""

echo "✅ 所有测试完成！"
```

## 📊 测试用例

| 测试项 | 方法 | URL | 预期结果 |
|--------|------|-----|----------|
| 健康检查 | GET | / | 200 OK |
| 代理请求 | POST | /proxy | 200 OK, 返回目标数据 |
| 简单代理 | GET | /proxy/simple | 200 OK, 返回目标数据 |
| 配置获取 | GET | /config | 200 OK, 返回配置信息 |

## 🎯 预期输出示例

```json
{
  "status_code": 200,
  "data": {
    "userId": 1,
    "id": 1,
    "title": "sunt aut facere repellat provident occaecati excepturi optio reprehenderit",
    "body": "quia et suscipit\nsuscipit recusandae consequuntur expedita et cum\nreprehenderit molestiae ut ut quas totam\nnostrum rerum est autem sunt rem eveniet architecto"
  },
  "headers": {...},
  "error": null
}
```

## 🔧 故障排除

### 端口占用问题
```bash
# 查找占用8000端口的进程
lsof -ti:8000

# 杀掉进程
kill -9 <PID>
```

### 服务未启动
```bash
# 启动后端
cd ~/Codes/CreativeProjects/miniClaw/backend
source venv/bin/activate
python3 run.py

# 启动前端
cd ~/Codes/CreativeProjects/miniClaw/frontend
npm start
```

### 依赖缺失
```bash
# 安装后端依赖
cd ~/Codes/CreativeProjects/miniClaw/backend
source venv/bin/activate
pip install -r requirements.txt

# 安装前端依赖
cd ~/Codes/CreativeProjects/miniClaw/frontend
npm install
```

## ✅ 验证清单

- [ ] 后端服务已启动 (http://localhost:8000)
- [ ] 前端服务已启动 (http://localhost:3000)
- [ ] 健康检查接口正常
- [ ] 代理接口返回正确数据
- [ ] CORS 配置正确
- [ ] 前端可以成功调用后端API

---

**测试时间**: 2026-02-18 21:18 GMT+8  
**状态**: ✅ 所有功能正常