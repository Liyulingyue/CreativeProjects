# FastOpenCodeConfig — 将 opencode 注册为开机启动服务

本工具集用于简化 `opencode` 在 Linux 系统下的开机启动配置。只需运行一键脚本，即可将 `opencode web --hostname 0.0.0.0 --port 4096` 注册为 systemd 服务。

---

## 1. 安装与前提

### 官方脚本安装
如果您尚未安装 `opencode`，可使用如下命令快速安装：
```bash
curl -fsSL https://opencode.ai/install | bash
```

### 验证安装
确保命令可被正确查找到：
```bash
opencode --version
# 或
which opencode
```

---

## 2. 快速部署 (推荐)

在该目录下运行我们提供的自动化脚本。它会自动执行以下操作：
- 通过 `which` 找到 `opencode` 的绝对路径
- 获取当前系统用户名和工作目录
- 生成定制化的 systemd 配置文件并安装

```bash
chmod +x setup_service.sh
./setup_service.sh
```

---

## 3. 手动部署指南

如果您偏好手动操作或需要更高级的配置，请执行以下步骤：

### 1) 测试启动
确保您的网络或端口设置正常：
```bash
opencode web --hostname 0.0.0.0 --port 4096
```

### 2) 身份验证配置 (可选)
支持通过环境变量配置账户和密码保护服务器访问：
- `OPENCODE_SERVER_PASSWORD`：设置访问密码
- `OPENCODE_SERVER_USERNAME`：设置用户名（默认为 opencode）

示例：
```bash
OPENCODE_SERVER_PASSWORD=secret opencode web --hostname 0.0.0.0 --port 4096
```

### 2) 创建服务文件
创建文件 `/etc/systemd/system/opencode.service` (需要 sudo 权限)：

```ini
[Unit]
Description=OpenCode Web Service
After=network.target

[Service]
Type=simple
# 替换为运行服务的系统用户名
User=youruser
# 设置工作目录
WorkingDirectory=/home/youruser
# 指向 opencode 的绝对路径（如 /usr/bin/opencode）
ExecStart=/usr/bin/opencode web --hostname 0.0.0.0 --port 4096
Restart=on-failure
RestartSec=5s
# 身份验证配置 (可选)
Environment=OPENCODE_SERVER_USERNAME=opencode
Environment=OPENCODE_SERVER_PASSWORD=your_password

[Install]
WantedBy=multi-user.target
```

### 3) 启用并启动
```bash
sudo systemctl daemon-reload
sudo systemctl enable opencode.service
sudo systemctl start opencode.service
```

---

## 4. 管理与日志

- **状态检查**：`sudo systemctl status opencode`
- **查看日志**：`sudo journalctl -u opencode -f`
- **停止服务**：`sudo systemctl stop opencode`

---

## 注意事项

- **虚拟环境**：如果 `opencode` 安装在虚拟环境，请手动修改 `ExecStart` 的路径。
- **端口访问**：默认端口 `4096` 超过 1024，无需 root 即可运行，但确保您的防火墙已开放该端口。
- **身份验证**：推荐配置 `OPENCODE_SERVER_USERNAME` 和 `OPENCODE_SERVER_PASSWORD` 环境变量保护服务器访问安全。

---

## 故障排查

若服务无法启动：
1. 检查 `journalctl` 日志中的报错信息。
2. 确认 `User` 字段的用户确实存在，且对 `WorkingDirectory` 有读写权限。
3. 如果 `which opencode` 返回为空，请手动指定 `ExecStart` 的全路径。
