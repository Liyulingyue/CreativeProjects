# AIWallpaper PRO 🎨

AIWallpaper 是一款基于 Rust 开发的智能桌面动态壁纸引擎，它能够根据您的创意提示词，实时调用 **ERNIE-Image (文心一言绘图引擎)** 生成独一无二的艺术壁纸，并将其完美嵌入到 Windows 桌面底层。

![App Icon](assets/app_icon.png)

## ✨ 核心特性

- **智能创作**：内置 10+ 种艺术风格预设，支持通过随机灵感骰子一键生成精美提示词。
- **动态注入**：采用类似 Lively Wallpaper 的 `0x052C` 消息钩子技术，将渲染层注入到桌面图标下方。
- **极速渲染**：基于 WebView2 GPU 加速，支持平滑的壁纸切换过渡动画。
- **科技感 UI**：Glassmorphism (毛玻璃) 风格控制面板，支持托盘隐藏与任务栏优化。
- **开机粒子特效**：在首张壁纸生成前展示动态粒子背景与品牌标识，告别黑屏闪烁。

## 🚀 快速开始

### 1. 环境准备
- 确保您的系统为 **Windows 10/11**。
- 已安装 [Rust 编译环境](https://rustup.rs/)。
- 已安装 WebView2 运行时（通常 Win10/11 已自带）。

### 2. 获取 API Token
前往 [Baidu AI Studio 访问令牌页](https://aistudio.baidu.com/account/accessToken) 获取您的 `AccessToken`。将其填入软件“设置”面板中的 API Token 输入框中。

### 3. 开发环境运行
```powershell
cargo run
```

## 📦 如何生成发布版本 (Release)

为了获得最佳性能并隐藏调试控制台，请使用以下命令进行编译：

```powershell
# 编译 Release 版本
cargo build --release
```

编译完成后，您可以在以下位置找到可执行文件：
`target/release/ai-wallpaper.exe`

这是一个单文件绿色版，您可以将其移动到任何地方运行。

## 🛠️ 技术架构

- **后端**: Rust (Tao, Wry, Tokio)
- **前端**: Tailwind CSS, Lucide Icons, Canvas API
- **注入方案**: Win32 API (`SendMessageTimeout`, `SetParent`)
- **图标展示**: `embed-resource` (嵌入 .exe) + `image` crate (托盘 RGBA 渲染)

## 🙏 特别鸣谢

本项目向以下优秀的作品和平台表示由衷的感谢：

- **[Lively Wallpaper](https://github.com/rocksdanister/lively)**：提供了极其宝贵的 Windows 桌面注入思路和 `WorkerW` 寻找逻辑，是本项目能够兼容 Win11 24H2 的核心启蒙。
- **[百度文心一言 (ERNIE-Image)](https://aistudio.baidu.com/)**：提供了强大的生成式 AI 绘图能力，让每一张壁纸都充满了无限可能。
- **[Tao/Wry 团队](https://github.com/tauri-apps/wry)**：提供了优秀的跨平台底层窗口与 WebView 桥接库。

---

*Made with ❤️ by AI + Human Collaboration*
