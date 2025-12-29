# 使用官方Python镜像作为基础镜像
FROM python:3.11-slim

# 设置工作目录
WORKDIR /app

# 复制requirements.txt并安装依赖
COPY requirements.txt .
RUN pip install --no-cache-dir -r requirements.txt

# 复制项目文件
COPY . .

# 下载模型（在构建时下载以便缓存）
RUN modelscope download --model mixedbread-ai/mxbai-rerank-base-v1 --local_dir mixedbread-ai/mxbai-rerank-base-v1

# 暴露端口
EXPOSE 10053

# 运行应用
CMD ["python", "fastapi_rerank_server.py"]