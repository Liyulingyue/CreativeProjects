# Thumbs 缓存优化待办

## 背景
当前缩略图缓存目录为 `data/thumbs`，默认会随着浏览图片数量持续增长，缺少容量治理和失效清理机制。

## 目标
- 控制缩略图缓存体积，避免无上限增长。
- 降低缩略图键冲突风险，避免不同目录图片出现串图。
- 提供可观测、可手动触发的清理能力。

## 范围
1. 缓存回收策略
- 支持按最大容量上限清理（例如默认 1GB，可配置）。
- 支持按 TTL 清理（例如默认 30 天，可配置）。
- 推荐优先实现 LRU（最近最少使用）策略。

2. 缩略图命名与键设计
- 由当前基于 `stem + size` 的命名改为基于“规范化绝对路径 + 文件大小 + 修改时间”计算哈希。
- 保留固定后缀（如 `.jpg`），便于工具识别。

3. 失效文件清理
- 当原图不存在时，缩略图可标记为失效并清理。
- 提供一次性全量扫描清理入口。

4. 运维与使用入口
- 增加手动清理接口（API）和前端按钮（设置页或缓存页）。
- 提供只统计不删除的 dry-run 模式。

## 验收标准
1. 容量受控
- 设置上限后，缓存目录体积稳定在上限附近（允许短时波动）。

2. 一致性
- 同名同大小但不同目录图片，不再出现缩略图冲突。

3. 可恢复性
- 删除缓存后再次浏览可自动重建，不影响主流程。

4. 可观测性
- 提供缓存统计信息：总文件数、总大小、最近清理时间、最近清理回收大小。

## 实施建议（分期）
1. P1（高优先）
- 键改为路径哈希。
- 增加最大容量 + LRU 清理。

2. P2
- 增加 TTL 清理。
- 增加失效原图检测与清理。

3. P3
- 前端可视化缓存管理页面与手动操作入口。

## 风险与注意事项
- Windows 与 Linux 路径规范化策略不同，哈希前需统一规则。
- 并发生成缩略图时要避免重复写入和竞争删除。
- 清理任务建议限速或分批，避免阻塞主请求。

## 前端交付方式统一：运行时可配置前端来源

### 背景
当前 `rust/` crate 的 `embed-frontend` feature 在编译时将前端 `dist/` 嵌入二进制，Tauri 则通过 `frontendDist` 独立打包前端。若 Tauri 构建同时启用 `embed-frontend`，会导致同一份前端资源被嵌入两次（体积浪费）；若不启用，Tauri 的 HTTP 模式（`PHOTO_ANALYZER_EXPOSE_HTTP=1`）无法 serve 前端给局域网其他用户。

### 目标
- 前端来源从编译时决定改为运行时可配置，消除重复嵌入。
- Tauri HTTP 模式可复用 Tauri 已打包的前端资源，支持局域网分享。
- 独立 exe（无 Tauri）仍通过 `embed-frontend` 内嵌前端。

### 方案
`build_app()` 增加 `frontend_dir: Option<PathBuf>` 参数，运行时决定前端来源：

```
优先级：
1. frontend_dir 有值 → ServeDir 从磁盘读（Tauri 场景：指向 Tauri resource_dir）
2. frontend_dir 为空 + embed-frontend feature → rust-embed 编译时嵌入
3. frontend_dir 为空 + 无 embed-frontend → 只 serve API
```

### 改动范围
1. `rust/src/lib.rs`
   - `build_app(frontend_dir: Option<PathBuf>)` 加参数
   - `frontend_dir` 有值时用 `tower_http::services::ServeDir` fallback
   - `frontend_dir` 为空时走现有 `embed-frontend` / 提示逻辑
   - `run_server()`、`spawn_server()`、`InProcessApi::new()` 同步更新签名

2. `desktop/src/main.rs`
   - `PHOTO_ANALYZER_EXPOSE_HTTP` 分支中，获取 Tauri `resource_dir()` 拼接前端路径，传入 `spawn_server`
   - `desktop/Cargo.toml` 不再需要 `embed-frontend` feature

3. `desktop/tauri.conf.json`（可选）
   - 确认前端资源被打入 Tauri bundle 的 resource 目录

### 验收标准
- `cargo build -p photo_analyzer --features embed-frontend`：独立 exe，内嵌前端，可局域网分享
- `cargo build -p photo_analyzer_tauri`（不带 embed-frontend）：Tauri 桌面窗口 + HTTP 模式复用 Tauri 前端，可局域网分享
- 两种构建均不出现前端资源重复嵌入
- `spawn_server` 绑定地址支持 `0.0.0.0`（当前硬编码 `127.0.0.1`，需改为可配置，否则局域网不可达）

### 实施分期
1. P1
   - `build_app` 加 `frontend_dir` 参数，实现运行时 ServeDir 分支
   - `spawn_server` 支持配置绑定地址（`0.0.0.0` vs `127.0.0.1`）

2. P2
   - Tauri `setup()` 中获取 `resource_dir()` 并传入
   - 移除 `desktop/Cargo.toml` 对 `embed-frontend` 的依赖

3. P3
   - CLI 参数 `--host`、`--port`、`--frontend-dir` 替代环境变量
   - 文档更新

## CLI 分发与安装体验

### 背景
`rust/` + `embed-frontend` 编译出的独立 exe 已具备完整能力：启动 HTTP 服务器、内嵌前端、自动打开浏览器、局域网可访问。但当前缺少 CLI 参数支持和分发渠道，无法实现 `curl/apt install photoanalyzer` → 命令行启动 → 浏览器访问的体验。

### 现状
- 独立 exe：固定端口 `8001`（`PORT` 环境变量可改），绑定 `0.0.0.0`，自动打开浏览器
- Tauri HTTP 模式：随机端口（bind `:0`），绑定 `127.0.0.1`，仅本机可达

### 目标
- 支持 `photoanalyzer --port 8080 --host 0.0.0.0 --no-open` 等 CLI 参数
- 支持 `curl -fsSL .../install.sh | sh` 一键安装
- 支持 `apt install photoanalyzer`（.deb 包）
- 支持 Homebrew（macOS）
- 可选：systemd unit file 实现开机自启

### 改动范围
1. `rust/Cargo.toml`
   - 增加 `clap` 依赖（可选 feature 或默认）
2. `rust/src/main.rs`
   - 解析 CLI 参数：`--port`、`--host`、`--no-open`、`--frontend-dir`
   - 替代当前环境变量方式
3. CI / GitHub Actions
   - 跨平台构建矩阵：`x86_64-linux-gnu`、`x86_64-apple-darwin`、`x86_64-pc-windows-msvc`
   - 产物上传至 GitHub Release
4. 安装脚本
   - `install.sh`：检测平台 → 下载对应二进制 → 放入 `/usr/local/bin/`
5. 打包
   - `cargo deb` 生成 .deb
   - Homebrew formula
   - 可选：`.rpm`、`aur`

### 验收标准
- `photoanalyzer` 启动后浏览器自动打开，局域网设备可通过 `http://<ip>:<port>` 访问
- `photoanalyzer --port 9000 --host 0.0.0.0 --no-open` 在 9000 端口启动，不自动打开浏览器
- `curl | sh` 一条命令完成安装
- `apt install photoanalyzer` 可用（至少 Ubuntu/Debian）

### 实施分期
1. P1
   - CLI 参数解析（clap）
   - `--port`、`--host`、`--no-open` 支持

2. P2
   - GitHub Actions CI 跨平台构建 + Release
   - `install.sh` 安装脚本

3. P3
   - `.deb` 包 + apt 仓库
   - Homebrew formula
   - systemd unit file

## 可选扩展
- 支持按目录维度统计缓存占用。
- 支持按图片类型（jpg/png/webp）分组统计。
- 支持导出缓存诊断报告。
