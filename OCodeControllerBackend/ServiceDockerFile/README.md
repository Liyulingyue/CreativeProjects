# OpenCode Service Docker Image

基于 `ubuntu:22.04` 的 OpenCode 服务镜像，封装了 opencode web 服务，供后端零部署功能使用。

## 镜像信息

- **基础镜像**: `ubuntu:22.04`
- **暴露端口**: `4096`
- **环境变量**:
  - `OPENCODE_SERVER_USERNAME` - 访问用户名（默认 `opencode`）
  - `OPENCODE_SERVER_PASSWORD` - 访问密码（启动时必须指定）

## 构建

```bash
cd /home/liyulingyue/Codes/CreativeProjects/OCodeControllerBackend/ServiceDockerFile
docker build -t opencode-service:latest .
```

## 本地测试

```bash
docker run -d -p 4098:4096 --name opencode-test \
  -e OPENCODE_SERVER_USERNAME=test \
  -e OPENCODE_SERVER_PASSWORD=123456 \
  opencode-service:latest
```

访问 `http://localhost:4098`，使用 `test / 123456` 登录。

## 清理测试容器

```bash
docker rm -f opencode-test
```
