# 代理配置速查（Quick proxy setup）

说明：这是一个便捷参考，包含常见工具与系统级别的 HTTP / HTTPS / SOCKS5 代理配置示例。

> 示例代理（替换为你的地址/端口/凭证）：
> - 主机：`192.168.3.6`
> - 端口：`7890`

---

## 快速 — 当前 shell（临时，重启失效）

- HTTP/HTTPS（常用）

```bash
export http_proxy="http://192.168.3.6:7890"
export https_proxy="http://192.168.3.6:7890"
# 避免局域网走代理
export NO_PROXY="localhost,127.0.0.1,::1,192.168.0.0/16"
```

- SOCKS5（如果你的代理是 socks5，例如 Clash/ss-local）

```bash
export ALL_PROXY="socks5://192.168.3.6:7890"
# curl（确保 DNS 通过代理解析）
curl --socks5-hostname 192.168.3.6:7890 https://raw.githubusercontent.com
```

---

## 持久化（单用户）

将上面的 `export` 行添加到 `~/.profile` 或 `~/.bashrc`：

```bash
cat >> ~/.profile <<'EOF'
export http_proxy="http://192.168.3.6:7890"
export https_proxy="http://192.168.3.6:7890"
export ALL_PROXY="socks5://192.168.3.6:7890"   # 如果需要
export NO_PROXY="localhost,127.0.0.1,::1,192.168.0.0/16"
EOF
# 然后载入
source ~/.profile
```

---

## 系统级（所有用户 / 服务）

- 将环境变量写入 `/etc/environment`（需要 sudo）：

```bash
sudo tee /etc/environment >/dev/null <<EOF
http_proxy="http://192.168.3.6:7890"
https_proxy="http://192.168.3.6:7890"
ALL_PROXY="socks5://192.168.3.6:7890"
NO_PROXY="localhost,127.0.0.1,::1,192.168.0.0/16"
EOF
```

- apt（系统包管理器）：

```bash
sudo tee /etc/apt/apt.conf.d/95proxies >/dev/null <<EOF
Acquire::http::Proxy "http://192.168.3.6:7890/";
Acquire::https::Proxy "http://192.168.3.6:7890/";
EOF
```

---

## 常用工具配法

- curl（临时）

```bash
# HTTP 代理
curl -x http://192.168.3.6:7890 https://raw.githubusercontent.com
# SOCKS5（DNS 通过代理）
curl --socks5-hostname 192.168.3.6:7890 https://raw.githubusercontent.com
```

- git

```bash
git config --global http.proxy  http://192.168.3.6:7890
git config --global https.proxy http://192.168.3.6:7890
# 取消
# git config --global --unset http.proxy
```

- npm / yarn

```bash
npm config set proxy http://192.168.3.6:7890
npm config set https-proxy http://192.168.3.6:7890
# yarn
yarn config set proxy http://192.168.3.6:7890
```

- Homebrew（Linux）：读取环境变量 `http_proxy` / `https_proxy`

- systemd 服务（为单个 service 添加代理）

```ini
# /etc/systemd/system/your.service.d/http-proxy.conf
[Service]
Environment="http_proxy=http://192.168.3.6:7890" "https_proxy=http://192.168.3.6:7890"
```
然后运行：
```bash
sudo systemctl daemon-reload
sudo systemctl restart your.service
```

- Docker 守护进程（daemon.json）

```json
{
  "proxies": {
    "default": {
      "httpProxy": "http://192.168.3.6:7890",
      "httpsProxy": "http://192.168.3.6:7890",
      "noProxy": "localhost,127.0.0.1"
    }
  }
}
```

---

## 测试代理是否生效

```bash
# 网络请求测试
curl -I https://raw.githubusercontent.com
# 检查环境变量
env | grep -i proxy
# git 测试
git ls-remote https://github.com/git/git.git
# apt 测试
sudo apt update
```

---

## 取消 / 清理代理

```bash
# 仅当前 shell
unset http_proxy https_proxy ALL_PROXY
# 删除 ~/.profile 中的行并重新登录或 source
# apt 的配置：sudo rm /etc/apt/apt.conf.d/95proxies
```

---

## 注意事项 & 故障排查

- SOCKS5 与 DNS：使用 `--socks5-hostname` 可让 DNS 解析也通过代理。
- 需要认证的代理：格式为 `http://user:pass@host:port`（注意凭证安全）。
- 若 curl 连接超时，尝试 `curl -v` / `curl --proxy` 指定不同类型代理进行排查。
- 某些 GUI 或 system service 可能不使用用户的 shell 环境变量，需配置 systemd 或 /etc/environment。

---

## 参考命令（快速粘贴）

```bash
# 临时（HTTP）
export http_proxy="http://192.168.3.6:7890"; export https_proxy="http://192.168.3.6:7890"
# 测试
curl -I https://raw.githubusercontent.com
```

---

如果需要，我可以：
- 把你选择的那套配置写入 `~/.profile` 或 `/etc/environment`；
- 或者为某个服务（systemd）创建 drop-in 文件。 

（你后续想把配置放到何处，可以告诉我，我来为你写好。）
