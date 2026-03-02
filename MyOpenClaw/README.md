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
OpenClaw 需要通过交互式命令行设置 AI 提供商、API 密钥等信息。运行以下命令进入设置界面：
```bash
docker-compose run --rm openclaw-cli setup
```
> **注意**：由于配置了 Docker 卷挂载，你在交互界面中输入的配置将持久化保存在本地的 ./config 目录中。即使该临时配置容器在完成后被删除 (--rm)，配置数据依然保留。

### 3. 启动主服务
配置完成后，启动 OpenClaw 网关服务：
```bash
docker-compose up -d
```
启动后，可以通过 http://localhost:18789 访问网关。

### 4. 目录说明
- config/: 存放 OpenClaw 的所有配置文件和凭据。
- workspace/: 存放 AI 生成的文件和工作空间内容。
- .env: 环境变量配置（由 init.sh 自动生成）。

### 5. 常用维护命令
- **查看日志**: docker-compose logs -f
- **停止服务**: docker-compose down
- **重新进入交互配置**: docker-compose run --rm openclaw-cli setup (配置会自动更新到 config 目录)
