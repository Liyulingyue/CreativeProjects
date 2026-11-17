# WindowsRegGenerator

Windows 自定义 **URL 协议处理器（URL Protocol Handler）** 注册表生成与清理工具。

把任意 EXE 注册为一个自定义协议（例如 `myapp://`），网页上的链接就能直接唤起该程序；
同时可扫描并删除本机已注册的协议项，用于“装完即走”的卸载场景。

## ✨ 核心特性

- **图形界面**：纯 Rust / egui 单体程序，无 Node、无 Python 依赖，双击即用。
- **生成 .reg 文件**：一键生成标准 `.reg`（UTF-16LE + BOM），双击即可导入注册表。
- **一键清理**：扫描 `HKCU` / `HKLM` 下与指定前缀相关的协议键，支持逐项勾选删除。
- **权限自适应**：非管理员时提示“访问被拒绝”，并提供“以管理员身份重启”按钮（UAC 提权）。
- **可选写入项**：`DefaultIcon`（图标）、`FriendlyTypeName`（浏览器中显示的名称）。
- **附带 CLI**：`generate` / `clean` 两个 example，便于脚本化与打包流程使用。

## 📖 工作原理

Windows 通过注册表中的 `URL Protocol` 空值来识别自定义协议。注册后：

- 在浏览器地址栏输入 `myapp://任意内容` → 唤起目标 EXE，并把整段 URL 作为 `%1` 传入命令行；
- 在网页中放置 `<a href="myapp://xxx">` 链接 → 点击后唤起目标 EXE。

本工具生成的键结构如下（以 `HKCU` 为例）：

```text
HKEY_CURRENT_USER\Software\Classes\myapp
    (Default)          = "URL:myapp Protocol"
    "URL Protocol"     = ""
    "DefaultIcon"      = "C:\Program Files\App\app.exe,0"
    "FriendlyTypeName" = "我的应用"

HKEY_CURRENT_USER\Software\Classes\myapp\shell\open\command
    (Default)          = "\"C:\Program Files\App\app.exe\" \"%1\""
```

`HKEY_CLASSES_ROOT` 是 `HKCU\Software\Classes` 与 `HKLM\SOFTWARE\Classes` 的合并视图，
因此选择「所有用户 (HKLM)」即可实现全机生效（导入与清理都需要管理员权限）。

## 🛠️ 使用

### 图形界面

```bash
cargo run --release
```

1. **① 注册目标**
   - *协议前缀*：填 `myapp`，界面右侧会实时显示 `myapp://`；
     前缀需以字母开头，仅允许字母、数字、`+`、`-`、`.`（符合 RFC 的 scheme 语法）。
   - *EXE 路径*：点「浏览…」选择目标程序，或直接粘贴路径。
   - *注册范围*：`当前用户 (HKCU)` 无需管理员；`所有用户 (HKLM)` 全机生效。
2. **② 生成 .reg 文件**
   - 「保存为 .reg…」→ 选择保存位置 → 双击生成的 `.reg` 完成导入；
   - 「复制内容」可把 .reg 文本复制到剪贴板。
3. **③ 清理本机已注册的前缀**
   - 「扫描注册表」列出 `HKCU` / `HKLM` 中匹配的协议键（显示是否含 `URL Protocol` 标记与实际命令行）；
   - 勾选后点「删除选中的 N 项」，二次确认即彻底清除该协议注册。

### 命令行

```bash
# 生成 .reg（默认输出 <前缀>.reg）
cargo run --release --example generate -- --scheme myapp --exe "C:\Tools\myapp.exe"

# 指定输出路径、作用域、友好名称
cargo run --release --example generate -- \
    --scheme myapp --exe "C:\Tools\myapp.exe" --out "D:\myapp.reg" \
    --scope all-users --friendly-name "我的应用"

# 扫描并清理（--dry-run 仅查看不删除）
cargo run --release --example clean -- --scheme myapp --dry-run
cargo run --release --example clean -- --scheme myapp

# 删除所有以 myapp 开头的其它协议
cargo run --release --example clean -- --scheme myapp --prefix-match
```

| 参数 | 说明 |
| --- | --- |
| `--scheme` | 协议前缀，如 `myapp` |
| `--exe` | 目标 EXE 路径（必须是存在的 .exe） |
| `--out` | 输出 `.reg` 路径，默认 `<前缀>.reg` |
| `--scope` | `current-user`（默认，HKCU）/ `all-users`（HKLM） |
| `--friendly-name` | 写入 `FriendlyTypeName` 的显示名 |
| `--no-icon` | 不写入 `DefaultIcon` |
| `--prefix-match` | 清理时同时匹配以该前缀开头的其它协议 |

## 🧱 项目结构

```text
WindowsRegGenerator/
├── Cargo.toml
├── examples/
│   ├── clean.rs          # CLI：扫描并删除协议注册项
│   └── generate.rs       # CLI：生成 .reg 文件
└── src/
    ├── app.rs            # egui 界面
    ├── elevate.rs        # UAC 提权重启
    ├── lib.rs            # 库入口（供 example 复用）
    ├── main.rs           # 可执行入口
    ├── regfile.rs        # .reg 文本渲染 + UTF-16LE BOM 编码
    ├── registry.rs       # winreg 扫描 / 递归删除
    └── scheme.rs         # 协议配置模型、键路径构造与校验
```

## 📦 构建

```bash
cargo build --release          # target/release/windows-reg-generator.exe
cargo test                     # 单元测试
cargo clippy --all-targets     # 静态检查
cargo fmt                      # 格式化
```

- 依赖：`eframe 0.37`（egui 渲染后端固定为 **glow**，兼容性优于 wgpu）、`winreg 0.56`、`rfd 0.15`、`windows-sys 0.61`。
- 界面文案为中文，已启用 `system_fonts`，直接使用系统字体渲染 CJK 字形。

## ⚠️ 注意事项

- **`.reg` 必须是 UTF-16LE 且带 BOM**，否则 `regedit` 可能拒绝导入或中文路径乱码；本工具保存时已自动处理。
- **浏览器策略**：Chrome / Edge 对「http 页面跳转到自定义协议」通常要求用户手势，并可能弹出「是否允许打开应用」的确认框；部分浏览器版本会直接拦截。这属于浏览器安全策略，与本工具无关。
- **删除不可撤销**：清理功能会递归删除 `Classes\<前缀>` 整个键（含 `shell\open\command`）。建议先用 `--dry-run` 或界面里的扫描结果确认目标。
- **HKLM 需要管理员**：删除 `HKEY_LOCAL_MACHINE\SOFTWARE\Classes\<前缀>` 必须提权；工具会在检测到权限不足时提示并提供提权重启按钮。
- 只清理由本工具创建的协议项；同一键下若有其他程序写入的子键，会一并删除。

## 📄 License

MIT
