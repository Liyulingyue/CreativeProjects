将ZeroClaw的配置文件中 cat ~/.zeroclaw/config.toml 的内容进行必要修改

从
```toml
[gateway]
port = 3000
host = "127.0.0.1"
require_pairing = true
allow_public_bind = false
paired_tokens = []
pair_rate_limit_per_minute = 10
webhook_rate_limit_per_minute = 60
idempotency_ttl_secs = 300
```

修改为
```toml
[gateway]
port = 3000
host = "0.0.0.0"
require_pairing = false
allow_public_bind = true
paired_tokens = []
pair_rate_limit_per_minute = 10
webhook_rate_limit_per_minute = 60
idempotency_ttl_secs = 300
```

重新加载 systemctl --user restart zeroclaw.service && sleep 2 && journalctl --user -u zeroclaw.service -n 10 --no-pager

然后运行本项目即可。

### 运行前端界面

项目提供了一个基于 React 的本地对话页面 `openclaw-frontend`。

1. 进入目录：`cd openclaw-frontend`
2. 安装依赖：`npm install`
3. 启动开发服务器：`npm run dev`
4. 访问界面：默认在 `http://localhost:5173`

界面功能：
- **健康检查**：自动检测 ZeroClaw Gateway 状态。
- **单轮对话**：向 `/webhook` 发送指令并获取响应。
- **本地历史**：自动保存对话记录到浏览器，支持导出和清除。
- **配对支持**：支持输入 `X-Pairing-Code` 进行身份验证。

```text
Feb 18 00:10:48 WalnutPi zeroclaw[23533]: 🧠 ZeroClaw daemon started
Feb 18 00:10:48 WalnutPi zeroclaw[23533]:    Gateway:  http://0.0.0.0:3000
Feb 18 00:10:48 WalnutPi zeroclaw[23533]:    Components: gateway, channels, heartbeat, scheduler
Feb 18 00:10:48 WalnutPi zeroclaw[23533]:    Ctrl+C to stop
Feb 18 00:10:48 WalnutPi zeroclaw[23533]: 🦀 ZeroClaw Gateway listening on http://0.0.0.0:3000
Feb 18 00:10:48 WalnutPi zeroclaw[23533]:   POST /pair      — pair a new client (X-Pairing-Code header)
Feb 18 00:10:48 WalnutPi zeroclaw[23533]:   POST /webhook   — {"message": "your prompt"}
Feb 18 00:10:48 WalnutPi zeroclaw[23533]:   GET  /health    — health check
Feb 18 00:10:48 WalnutPi zeroclaw[23533]:   ⚠️  Pairing: DISABLED (all requests accepted)
Feb 18 00:10:48 WalnutPi zeroclaw[23533]:   Press Ctrl+C to stop.
```