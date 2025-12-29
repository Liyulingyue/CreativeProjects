# Rerank API

基于 FastAPI 的重排接口，兼容 OpenAI API 格式，使用本地 CrossEncoder 模型进行文档重排。便于在腾讯开源的Rag框架 WeKnora 中使用。

## 安装依赖

```bash
pip install -r requirements.txt
```

## 获取模型

下载 mixedbread-ai/mxbai-rerank-base-v1 模型：https://www.modelscope.cn/models/mixedbread-ai/mxbai-rerank-base-v1

```bash
pip install modelscope
modelscope download --model mixedbread-ai/mxbai-rerank-base-v1 --local_dir mixedbread-ai/mxbai-rerank-base-v1
```

## 运行服务器

```bash
python fastapi_rerank_server.py
```

服务器将在 `http://localhost:10053` 启动。
## 使用Docker运行

### 构建和运行Docker容器

```bash
# 构建镜像
docker build -t rerank-server .

# 运行容器
docker run -p 10053:10053 rerank-server
```

### 使用Docker Compose

```bash
# 启动服务
docker-compose up -d

# 停止服务
docker-compose down
```
## API 端点

### POST /v1/rerank

重排文档的端点，兼容 OpenAI API 格式。

**请求参数：**

```json
{
  "model": "mixedbread-ai/mxbai-rerank-base-v1",
  "query": "查询文本",
  "documents": ["文档1", "文档2", "文档3"],
  "parameters": {
    "return_documents": true,
    "top_k": 3
  }
}
```

**响应格式：**

```json
{
  "id": "rerank-uuid",
  "object": "list",
  "model": "mixedbread-ai/mxbai-rerank-base-v1",
  "usage": {},
  "results": [
    {
      "index": 0,
      "relevance_score": 0.9968,
      "document": {
        "text": "文档内容"
      }
    }
  ]
}
```

### GET /health

健康检查端点。

**响应：**

```json
{
  "status": "ok"
}
```

## 测试

运行测试脚本：

```bash
python call_api.py
```

## 使用示例

### 使用requests库

```python
import requests

response = requests.post("http://localhost:10053/v1/rerank", json={
    "query": "Who wrote 'To Kill a Mockingbird'?",
    "documents": [
        "Harper Lee wrote 'To Kill a Mockingbird'",
        "Jane Austen wrote 'Pride and Prejudice'",
        "Some other book information"
    ]
})

results = response.json()["results"]
for result in results:
    print(f"Score: {result['relevance_score']}, Document: {result['document']['text']}")
```

## 致谢
参考：https://zhuanlan.zhihu.com/p/1946896467777288068