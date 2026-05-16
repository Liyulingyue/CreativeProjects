# OpenCode Harmony 开发待办事项

## 功能增强

### 对话逻辑对齐 opencode web
- **描述**: 对齐会话创建、promptAsync 发送、模型下发、SSE/消息刷新逻辑
- **状态**: 排查中
- **优先级**: 高
- **备注**:
  - App 当前会先插入本地 assistant 占位消息，导致显示顺序与 web 不一致
  - 需要对照 `references/opencode/packages/app/src/components/prompt-input/submit.ts`
  - 重点核对 `/session/{id}/prompt_async` 请求体：`agent`、`model`、`variant`、`messageID`、`parts`
  - 核对 `buildPromptBody()` 是否把模型错误地当成字符串下发
  - 核对 `promptMessageId` 是否与后端真实 user message ID 对齐
  - 核对 `fetchAndRefreshMessages()` 依赖 `promptIndex` 的逻辑是否会导致一直停留在“思考中...”

### 模型选择后无法正常对话
- **描述**: 自从支持选择模型后，发起对话后整体流程异常
- **状态**: 排查中
- **优先级**: 高
- **备注**:
  - `createSession()` 当前并未真正透传字符串模型到后端
  - 需要确认真实模型应在 create session 阶段还是 promptAsync 阶段生效
  - 需要参考 web 版 `model: { providerID, modelID }` 和 `variant` 的真实用法

### WebView 鉴权模式白屏
- **描述**: 带鉴权的后端进入 WebView 后白屏
- **状态**: 暂缓
- **优先级**: 中
- **备注**:
  - 已确认不是单纯鉴权失败，日志显示页面已加载到 `/?auth_token=...`
  - 当前已知问题包括：
    - 页面启动脚本访问 `localStorage.getItem()` 崩溃
    - 首页内联脚本被 CSP 拦截
  - 当前页面顶部已增加“暂不支持鉴权 WebView”提示

### 本地会话缓存
- **描述**: 会话数据同时保存在手机本地，便于离线查看和快速加载
- **状态**: 待实现
- **优先级**: 中
- **备注**:
  - 需要在 App 本地存储会话列表（可能只存会话元数据，如 id、name、backendId、directory）
  - 进入会话时从后端同步最新消息
  - 考虑添加"最后同步时间"字段
  - 参考 OpenCodeCore 的持久化模式

---

## 已完成功能

- 后端连接管理（添加/编辑/删除后端）
- 从后端获取会话列表
- 创建新会话
- 发送消息并接收响应
- 历史消息加载
