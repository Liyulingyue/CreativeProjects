# Scripts

这个目录放的是 PhotoAnalyzer 的打包脚本。

## 脚本会做什么

两个脚本执行的是同一套 2 步流程：

1. 构建前端资源
   - 在 web/frontend 下执行 npm run build
   - 生成 web/frontend/dist
2. 构建内嵌前端的 Rust Release 可执行文件
   - 在 rust 下执行 cargo build --release --features embed-frontend
   - 生成 rust/target/release/photo_analyzer.exe

任一步失败，脚本都会以非 0 状态退出。

## 行为说明

- 脚本通过 Scripts/.. 自动推导项目根目录，所以你从任意当前目录执行都可以。
- 前端构建必须先成功，否则不会继续 Rust 构建。
- Rust 构建使用 --features embed-frontend，会把前端 dist 打进二进制。
- 结束时会校验 photo_analyzer.exe 是否存在，并输出文件大小。
- 任一命令失败会立即退出，不会继续后续步骤。

## 关于 BAT 注释的编码兼容

- Windows `cmd.exe` 对批处理文件编码比较敏感。
- 在某些代码页下，`.bat` 里的中文注释即使写在 `REM` 后面，也可能被误解析为命令片段，出现乱码报错。
- 因此本仓库里的 `.bat` 注释使用 ASCII，避免误解析。
- 中文说明优先放在本 README 和 `build_embed_release.ps1` 里。

## 文件说明

- build_embed_release.bat
  - 适用于 Windows 命令行，或双击执行。
- build_embed_release.ps1
  - 适用于 PowerShell 执行。
- build_tauri_release.ps1
  - 构建真正的 Tauri 桌面应用（WebView 窗口），并自动打包后端可执行文件为资源。

## 使用方式

在 PhotoAnalyzer/Scripts 目录下执行：

### 方式 A：BAT

```bat
build_embed_release.bat
```

### 方式 B：PowerShell

```powershell
.\build_embed_release.ps1
```

如果执行策略阻止 ps1，请用：

```powershell
powershell -ExecutionPolicy Bypass -File .\build_embed_release.ps1
```

## 环境要求

- Node.js 和 npm 已安装，并在 PATH 中
- Rust 工具链已安装，并在 PATH 中（可用 cargo）
- 前端依赖已安装（web/frontend/node_modules 存在）

## 输出产物

主要输出文件：

- rust/target/release/photo_analyzer.exe

该可执行文件启动后会在根路径 / 提供内嵌前端页面。

## 启动行为说明

- Release 版在 Windows 下默认不显示控制台黑框。
- 启动 exe 后会默认自动打开浏览器访问 `http://localhost:8001`。
- 如需关闭自动打开，可设置环境变量：

```powershell
$env:PHOTO_ANALYZER_OPEN_BROWSER = "false"
```

## 典型发版流程

1. 打开终端并进入 PhotoAnalyzer/Scripts
2. 执行其中一个脚本（bat 或 ps1）
3. 等待出现 Build complete
4. 到 rust/target/release 下拿到 photo_analyzer.exe

## Tauri 桌面版构建（真正桌面窗口）

如果你希望像常规桌面软件一样运行（不是单纯打开浏览器），使用：

```powershell
.\build_tauri_release.ps1
```

脚本会执行：

1. 构建后端 `photo_analyzer.exe`
2. 复制到 `web/frontend/src-tauri/bin/photo_analyzer_backend.exe`
3. 执行 `npm run tauri:build`

产物目录：

- `web/frontend/src-tauri/target/release/bundle`
