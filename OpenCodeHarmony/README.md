# OpenCodeHarmony

OpenCodeHarmony 是一个基于 OpenHarmony/HarmonyOS 开发的项目，旨在将 [OpenCode](https://github.com/anomalyco/opencode.git) 的 Web 页面功能封装为真正的原生手机 App 体验。

## 项目目标

1.  **原生化资源 (Offline-First)**：将前端静态资源（JS/CSS/HTML）打包进 App 资源包（rawfile），实现秒级启动与离线访问。
2.  **前后端分离通信**：App 界面由本地提供，业务数据通过 REST/WebSocket 与远程 OpenCode 后端交互，通过配置动态指向后端 IP。
3.  **代码解耦与 JsBridge**：通过建立 `OpenCodeCore` 核心类与 `JsBridge` 隧道，实现 ArkTS 原生逻辑对 Web 页面的高度控制与数据注入。

## 目录结构

- `opencode/` (git submodule): 关联的 OpenCode 原始仓库。
- `entry/src/main/resources/rawfile/`: **前端资源阵地**。存放 OpenCode 编译后的 `dist` 产物。
- `entry/src/main/ets/core/`: **核心解耦层 (OpenCodeCore)**。封装了 `JsBridge` 接口（`OpenCodeApp`），包含后端连接管理、指令交互等。
- `entry/src/main/ets/pages/`: UI 视图层。
- `entry/src/main/ets/pages/BackendWebView.ets`: **内嵌浏览器页**，用于直接访问 opencode 后端服务。

## 核心逻辑：离线包模式

本项目采用 **"本地界面 + 远程服务"** 的混合架构：
- **数据流**：Web 页面加载本地资源 -> 通过 `window.OpenCodeApp` 桥接获取后端 IP -> 发起网络请求。
- **JsBridge 接口 (`OpenCodeApp`)**：
  - `getBackendUrl()`: 获取当前保存的后端服务器地址。
  - `storeBackendUrl(url)`: 更新并持久化保存后端服务路径。
  - `sendCommand(cmd)`: 发送原生指令。

## App 交互

App 包含三个 Tab：

### 会话 Tab
管理 opencode 会话列表，点击会话可进入详情页。

### 后端 Tab
管理 opencode 后端服务连接：
- **点击 [+]**：添加新后端（URL、Auth Token、备注）
- **点击后端项**：直接打开内嵌浏览器，访问 opencode Web UI
- **点击"配置"按钮**：编辑/删除后端配置

### 关于 Tab
App 版本信息和说明。

## 快速开始

1.  **初始化子模块**：
    ```bash
    git submodule update --init --recursive
    ```
2.  **编译前端资源**：
    进入 `opencode` 根目录（由于是 Monorepo 架构，建议在根目录安装依赖）：
    ```bash
    cd opencode
    bun install
    cd packages/app
    bun run build
    ```
3.  **资源收编**：
    将生成的 `dist` 目录内容拷贝到 `entry/src/main/resources/rawfile/`。
4.  **调试**：
    使用 DevEco Studio 运行到真机，在引导页输入电脑局域网 IP 即可。

## 架构优势

- **启动速度**：无需等待远程下载 JS 包，极速渲染。
- **安全性**：本地静态资源可防止被篡改。
- **扩展性**：未来可通过 `OpenCodeCore` 轻松接入鸿蒙原生的文件系统、摄像头等 API，为 AI 编程提供更强的端侧能力。
