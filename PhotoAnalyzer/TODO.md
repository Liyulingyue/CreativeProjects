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

## 前端交付方式：保留双重内嵌与 HTTP 服务

### 背景
当前 `rust/` crate 的 `embed-frontend` feature 会在编译时将前端 `dist/` 嵌入二进制，Tauri 也会把 `frontendDist` 打进自己的 bundle。桌面包里会存在两份同源前端资源，但这样可以同时保住 GUI 窗口和 HTTP 服务两条交付路径，且都不依赖额外的本地前端目录。

### 最终方案
- desktop 和 CLI 都保留前端内嵌，不再依赖运行时磁盘目录
- desktop 继续同时支持 GUI 启动和 HTTP 服务入口
- `--serve` 模式保持自包含，独立运行时不需要额外前端文件
- Tauri GUI 继续使用 `frontendDist`，保证桌面窗口路径稳定
- 代价：桌面发行物会有双重嵌入，但这是为了交付方式一致性和离线可用性

### 已完成改动
1. `rust/src/lib.rs`
   - 保持 `embed-frontend` 内嵌前端
   - 保持 `run_server()`、`spawn_server()`、`InProcessApi::new()` 的 HTTP 服务能力

2. `desktop/src/main.rs`
   - `--serve` 模式保留为 CLI 服务入口
   - `PHOTO_ANALYZER_EXPOSE_HTTP` 保留为桌面内置 HTTP 暴露模式
   - InProcess 模式保持 Tauri 自管前端

3. `desktop/Cargo.toml`
   - 启用 `embed-frontend` feature，确保桌面和 CLI 都能保持前端内嵌

### 验收标准
- `cargo build -p photo_analyzer --features embed-frontend`：独立 exe，内嵌前端，可局域网分享
- `cargo build -p photo_analyzer_tauri`：Tauri 桌面窗口 + HTTP 服务入口内嵌前端，可局域网分享
- 桌面发行物不依赖额外前端目录即可运行

### 实施分期
1. P1
   - 保持现有双重内嵌方案，统一说明与打包脚本

2. P2
   - 如需进一步减小体积，再评估是否拆分 GUI 和 CLI 产物

3. P3
   - 文档更新

## CLI 分发与安装体验

### 背景
`rust/` + `embed-frontend` 编译出的独立 exe 已具备完整能力：启动 HTTP 服务器、内嵌前端、自动打开浏览器、局域网可访问。但当前缺少 CLI 参数支持和分发渠道，无法实现 `curl/apt install photoanalyzer` → 命令行启动 → 浏览器访问的体验。

### 现状
- 独立 exe：支持 `--port`、`--host`、`--no-open` 等 CLI 参数，默认 `0.0.0.0:8001`，自动打开浏览器
- Tauri exe：保留 `--serve` 入口，无 `--serve` 则走 Tauri GUI
- Tauri HTTP 模式（`PHOTO_ANALYZER_EXPOSE_HTTP=1`）：随机端口（bind `:0`），绑定 `127.0.0.1`，仅本机可达

### 目标
- 支持 `photoanalyzer --port 8080 --host 0.0.0.0 --no-open` 等 CLI 参数
- 支持 `curl -fsSL .../install.sh | sh` 一键安装
- 支持 `apt install photoanalyzer`（.deb 包）
- 支持 Homebrew（macOS）
- 可选：systemd unit file 实现开机自启

### 改动范围
1. ~~`rust/Cargo.toml`~~ ✅ 已完成：增加 `clap` 依赖
2. ~~`rust/src/lib.rs`~~ ✅ 已完成：`CliArgs` 结构体 + `CliArgs::parse()`，`run_server()` 加 `host` 参数
3. ~~`rust/src/main.rs`~~ ✅ 已完成：调用 `CliArgs::parse()` 替代环境变量
4. ~~`desktop/src/main.rs`~~ ✅ 已完成：`--serve` 分发 + 复用 `CliArgs::parse()`
5. ~~`desktop/Cargo.toml`~~ ✅ 已完成：启用 `embed-frontend` feature + `tokio`
6. CI / GitHub Actions
   - 手动触发（`workflow_dispatch`），可选参数：构建目标（all / cli-only / tauri-only）
   - 跨平台构建矩阵：`x86_64-linux-gnu`、`x86_64-apple-darwin`、`x86_64-pc-windows-msvc`
   - Linux runner 预装 `libwebkit2gtk-4.1-dev`（Tauri 需要）
   - 产物上传至 GitHub Release
7. 安装脚本
   - `install.sh`：检测平台 → 下载对应二进制 → 放入 `/usr/local/bin/`
8. 打包
   - `cargo deb` 生成 .deb
   - Homebrew formula
   - 可选：`.rpm`、`aur`

### 分发策略
| 平台 | 产物 | 安装方式 |
|---|---|---|
| Windows | Tauri 安装包（含 `--serve` CLI 模式） | 双击安装，能力最全 |
| macOS | Tauri DMG（含 `--serve` CLI 模式） | 双击安装 |
| Linux 桌面 | Tauri AppImage（可选） | 下载运行 |
| Linux 服务器 | 独立 CLI 二进制（`rust/` + `embed-frontend`，无 Tauri 依赖） | `apt`/`curl` 安装 |

### 验收标准
- `photoanalyzer` 启动后浏览器自动打开，局域网设备可通过 `http://<ip>:<port>` 访问
- `photoanalyzer --port 9000 --host 0.0.0.0 --no-open` 在 9000 端口启动，不自动打开浏览器
- `curl | sh` 一条命令完成安装
- `apt install photoanalyzer` 可用（至少 Ubuntu/Debian）

### 实施分期
1. ~~P1~~ ✅ 已完成
   - ~~CLI 参数解析（clap）~~
   - ~~`--port`、`--host`、`--no-open` 支持~~
   - ~~`--serve` 分发（desktop）~~
   - ~~`embed-frontend` 启用（desktop）~~

2. P2
   - GitHub Actions CI（`workflow_dispatch` 手动触发）跨平台构建 + Release
   - `install.sh` 安装脚本

3. P3
   - `.deb` 包 + apt 仓库
   - Homebrew formula
   - systemd unit file

## 可选扩展
- 支持按目录维度统计缓存占用。
- 支持按图片类型（jpg/png/webp）分组统计。
- 支持导出缓存诊断报告。
