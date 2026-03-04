# MyOpenClaw
这个文件记录了我部署、使用OpenClaw的过程，主要是为了记录一些细节问题，方便以后回顾。

## Docker 部署指南

本项目已配置好基于 Docker 的一键初始化与部署方案。

### 1. 快速初始化
运行初始化脚本来生成 .env 配置文件并自动生成安全 Token：
```bash
chmod +x init.sh
./init.sh
```

### 2. 交互式配置 (关键步骤)
OpenClaw 需要通过交互式命令行设置 AI 提供商。请按以下“最稳妥”步骤操作：

**进入容器手动配置**
1. 确保服务已启动：`docker-compose up -d`
2. 进入容器：`docker-compose exec -it openclaw-gateway bash`
3. 手动运行配置：`node dist/index.js setup --force`
4. 完成后退出并重启针对配置生效：`exit && docker-compose restart openclaw-gateway`

> **注意**：由于配置了 Docker 卷挂载，配置将保存到本地的 ./config 目录中。

### 3. 访问与使用
启动后，可以通过浏览器访问网关：
- **地址**: `http://localhost:18789`
- **Token**: 查看 `.env` 文件中的 `OPENCLAW_GATEWAY_TOKEN`。

### 4. 目录说明
- `config/`: 存放所有配置文件和凭据。
- `workspace/`: 存放 AI 生成的文件。
- `.env`: 环境变量配置。

### 5. 常用维护命令
- **查看日志**: `docker-compose logs -f openclaw-gateway`
- **停止服务**: `docker-compose down`
- **重置所有配置**: `sudo rm -rf ./config/* && ./init.sh`

## 部署注意事项（避坑指南）
1. **绑定地址**: `OPENCLAW_GATEWAY_BIND` 必须设置为 `lan` 而不是 `0.0.0.0`。
2. **权限问题**: 如果启动失败提示 Permission Denied，运行 `sudo chown -R $USER:$USER ./config ./workspace`。
3. **Docker Compose 版本**: 建议安装 `docker-compose-plugin` (v2.x)，避免旧版 Python 的兼容性问题。
