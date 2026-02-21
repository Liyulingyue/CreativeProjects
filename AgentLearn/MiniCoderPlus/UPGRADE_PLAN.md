# MiniCoder Plus Workbench 升级计划

此文档描述为 MiniCoder Plus 添加“交互式终端 + Agent 对话”工作台模式的兼容性升级步骤。整个过程旨在**不破坏历史功能与数据**，所有新增均通过扩展接口和页面实现，保证现有 CLI 和 Web 聊天仍能无缝运行。

---

## 🧱 兼容性原则

1. **零破坏**：不修改现有 API `/chat` 的行为，CLI 命令保持不变。
2. **增量演进**：所有新逻辑封装到独立模块或路由中，可随时启用/停用。
3. **回滚简单**：如需回退，只需删除路由或切换配置，无任何数据库/文件迁移。
4. **用户分流**：新增工作台通过新页面（如 `/workbench`）访问，老用户使用 `/` 即可。

---

## 🚀 升级阶段与任务

### **阶段 1：后端持久化终端层 (PTY Manager)**
* 安装依赖 `pywinpty`（Windows）或 `ptyprocess`（跨平台）。
* 新建 `minicoder/core/terminal_manager.py`，提供 `TerminalSession` 类：
  * 启动一个常驻 Shell 进程（powershell/cmd/bash）。
  * 提供异步读写接口，支持多客户端共享。
  * 对旧版调用透明：不使用该会话仍可回退到 `subprocess.run`。

### **阶段 2：WebSocket 实时通讯接口**
* 在 `minicoder/server/app.py` 新增 `/ws/terminal/{session_id}`。
* WebSocket 管道实现：
  * **下行**：读 PTY 输出并推送给前端。
  * **上行**：接收前端按键/命令并写入 PTY。
* 该接口独立于 `/chat`，不会影响历史业务。

### **阶段 3：工具层适配**
* 重构 `minicoder/tools.py`，新增 `InteractiveCodeTools`。
* 在 `execute_bash` 内部切换逻辑：
  * 普通模式 → 直接调用 `subprocess.run`。
  * 交互模式 → 通过 `TerminalSession` 排队执行。
* 旧客户端继续使用原函数，兼容性保持。

### **阶段 4：前端架构升级**
* 引入 `react-router-dom`，重构成路由结构。
* 原来 `App.tsx` 内容迁移至 `src/routes/Home.tsx`。
* 新增 `src/routes/Workbench.tsx` 供工作台专用。
* 主页和工作台互不干扰，可同时部署。

### **阶段 5：工作台界面与 xterm 集成**
* 使用 `react-resizable-panels` 构建左右拖拽布局。
* 在工作台页面中加载 `xterm.js` 并连接 `/ws/terminal`。
* 前端代码只挂载在新页面，不影响现有聊天组件。

### **阶段 6：交互与夺权逻辑**
* 实现前端输入锁定/解锁机制：当 Agent 正执行时禁用键盘。
* Agent 在发送命令前可请求终端的最新输出，以保持上下文。
* 该逻辑完全在新页面实现，不会改动旧 UI。

---

## 🛠️ 技术架构设计 (Node.js & React)

除了上述阶段，针对终端稳定性和前端交互，我们采用以下深度设计：

### **1. 终端后端备选方案：Node.js PTY Bridge**
如果 Python 的 `pywinpty` 在处理 ANSI 转义字符或尺寸缩放时持续不稳定，计划引入轻量级 Node.js 中转：
* **核心库**：使用 `node-pty` (VS Code 同款库)，它对 Windows ConPTY 的支持是目前工业界最稳健的。
* **架构**：Node.js 负责 PTY 生命周期管理，通过 Socket/Named Pipe 与 Python 后端或前端 WebSocket 直接通讯。
* **意义**：彻底解决 Python 环境下常见的终端“阶梯效应”和字符渲染异常。

### **2. 前端 (React) 交互模型**
* **状态隔离**：Workbench 模式下，终端状态（Terminal Instance）与对话状态（Chat History）完全独立，避免 React 重新渲染导致终端重置。
* **XTerm 生态集成**：
  * `FitAddon`：配合 `ResizeObserver` 实现响应式布局。
  * `WebLinksAddon`：自动识别终端中的 URL。
  * `SearchAddon`：集成终端内容搜索功能。
* **防抖同步 (Debounced Sync)**：前端捕获到窗口缩放后，通过 100ms 防抖算法向后端同步 `cols` 和 `rows`，防止 PTY 进程由于频繁重绘而崩溃。

---

## ✅ 升级后验证要点
* CLI (`run.py cli`) 行为一致。
* 旧 Web 聊天不受影响。
* 新 `/workbench` 页面可启动并保持连接。
* Agent 在工作台和 CLI 均可运行。

---

> 💡 **兼容升级总结**：所有新增都是“侧楼扩建”，不改动老房子的结构。您可以在任意阶段停下来，也可以在生产环境部署两套并行服务。

以上是完整的升级计划，后续可以根据需要分步实施。