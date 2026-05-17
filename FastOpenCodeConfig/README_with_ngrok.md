# Ngrok 内网穿透配置指南

本指南帮助您为 OpenCode Web 服务配置 Ngrok 内网穿透，实现公网访问。

---

## 1. 安装 Ngrok

```bash
curl -sSL https://ngrok-agent.s3.amazonaws.com/ngrok.asc \
  | sudo tee /etc/apt/trusted.gpg.d/ngrok.asc >/dev/null \
  && echo "deb https://ngrok-agent.s3.amazonaws.com bookworm main" \
  | sudo tee /etc/apt/sources.list.d/ngrok.list \
  && sudo apt update \
  && sudo apt install ngrok
```

## 2. 配置认证令牌

1. 前往 [Ngrok Dashboard](https://dashboard.ngrok.com/get-started/setup/linux) 注册并获取认证令牌
2. 配置令牌：
   ```bash
   ngrok config add-authtoken <your_auth_token>
   ```

## 3. 身份验证配置

OpenCode 支持通过环境变量配置账户密码，保护服务访问安全：

| 环境变量 | 说明 | 默认值 |
|----------|------|--------|
| `OPENCODE_SERVER_USERNAME` | 用户名 | opencode |
| `OPENCODE_SERVER_PASSWORD` | 密码 | 无 |

### 快速测试

```bash
OPENCODE_SERVER_PASSWORD=secret opencode web --hostname 0.0.0.0 --port 4096
```

在另一个终端启动 Ngrok：
```bash
ngrok http 4096
```

## 4. 自动化部署 (推荐)

运行交互式脚本，一键配置 systemd 服务 + Ngrok 内网穿透：

```bash
chmod +x setup_service_ngrok.sh
./setup_service_ngrok.sh
```

该脚本会：
- 检测 opencode 和 ngrok 路径
- 询问配置端口、用户名、密码、Ngrok 认证令牌
- 生成并安装 systemd 服务
- 自动启动 Ngrok 内网穿透

## 5. 手动部署

### 1) 启动 OpenCode 服务

```bash
opencode web --hostname 0.0.0.0 --port 4096
```

### 2) 启动 Ngrok 穿透

```bash
ngrok http 4096
```

启动成功后会显示公网访问地址，例如：
```
Forwarding  https://xxxx.ngrok.io -> http://localhost:4096
```

## 6. 服务管理

- **查看 Ngrok 状态**：访问 http://localhost:4040
- **停止服务**：`sudo systemctl stop opencode`
- **查看日志**：`sudo journalctl -u opencode -f`

---

## 注意事项

- Ngrok 免费版每次重启后会更换公网地址
- 推荐配置账户密码防止未授权访问
- 确保防火墙允许 4096 端口访问
