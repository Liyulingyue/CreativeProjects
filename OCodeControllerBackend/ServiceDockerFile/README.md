# OpenCode Service Docker Image

基于 `ubuntu:22.04`，封装 OpenCode + FileBrowser 服务，通过 supervisor 管理。

## 服务

| 服务 | 端口 | 说明 |
|------|------|------|
| OpenCode | 4096 | AI 编程助手 |
| FileBrowser | 8080 | 文件管理（根目录 `/`，即容器根目录）|

## 环境变量

| 变量 | 说明 | 默认值 |
|------|------|--------|
| `OPENCODE_SERVER_USERNAME` | OpenCode 访问用户名 | `opencode` |
| `OPENCODE_SERVER_PASSWORD` | OpenCode 访问密码 | （空，需指定） |
| `FB_USERNAME` | FileBrowser 登录用户名 | `admin` |
| `FB_PASSWORD` | FileBrowser 登录密码 | `admin123` |

## 构建

```bash
cd /home/liyulingyue/Codes/CreativeProjects/OCodeControllerBackend/ServiceDockerFile
docker build -t opencode-service:latest .
```

## 本地测试

```bash
docker run -d -p 4098:4096 -p 8088:8080 --name opencode-test \
  -e OPENCODE_SERVER_USERNAME=test \
  -e OPENCODE_SERVER_PASSWORD=123456 \
  -e FB_USERNAME=admin \
  -e FB_PASSWORD=admin123 \
  opencode-service:latest

# OpenCode: http://localhost:4098（test / 123456）
# FileBrowser: http://localhost:8088（admin / admin123）
```

## 清理测试容器

```bash
docker rm -f opencode-test
```

## 推送

```bash
docker tag opencode-service:latest ghcr.io/anomalyco/opencode/opencode:latest
docker push ghcr.io/anomalyco/opencode/opencode:latest
```
