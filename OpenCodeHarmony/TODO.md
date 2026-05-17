# TODO - OpenCode Harmony 聊天功能开发计划

## 阶段 1：恢复核心通信逻辑

### 1a: 把 URL 从 prompt_async 改回 /session/{id}/message POST
- 修改 `sendMessage` 中的 URL

### 1b: extraData 直接传 JSON.stringify(body)
- 移除 `bodyStr` 中间变量
- 直接 `extraData: JSON.stringify(body)`

### 1c: 响应处理改回同步模式
- 等待 HTTP 200 响应
- 响应体直接是 AI 回复消息（OpenCodeMessage）
- 直接显示内容，不再轮询

### 1d: 恢复「思考中...」占位符气泡
- 发送前添加 `loadingId` 消息
- 收到响应后移除

### 1e: 移除轮询相关代码
- 移除 `startPolling()`
- 移除 `fetchAndRefreshMessages()`
- 移除 `fetchSessionStatus()`
- 移除 `fetchSessionInfo()`
- 移除 `fetchAgents()`
- 移除 `fetchProviders()`
- 移除 `scheduleSseReconnect()`
- 移除 `stopPolling()`
- 移除 `pollTimer` / `pollRetryCount`
- 移除 `promptAccepted`
- 移除 `pollTimeout` / `timeoutErrorMsg`

### 1f: 移除 SSE 相关代码
- 移除 `useSse`
- 移除 `sseReq` / `sseBuffer`
- 移除 `eventFetchTimer`
- 移除 `handleSseEvent()`
- 移除 `stopSse()`
- 移除 `scheduleEventFetchFallback()`
- 移除 SSE 的启动调用

## 阶段 2：逐步增加新功能

### 2a: 添加 agent 支持
- 添加 `agent: 'build'` 字段到请求体

### 2b: 添加 model 支持
- 从 project 获取 preferredModel
- 添加 model 字段到请求体

### 2c: 添加历史消息加载
- 页面加载时调用 loadHistory
- 显示历史消息

### 2d: 添加 abort 功能
- 添加 abort 按钮或功能
- 调用 POST /session/{id}/abort

### 2e: 添加超时处理
- 设置合理的超时时间
- 显示超时错误提示

---

## 参考：工作版本 (5f2bf04) 的核心逻辑

```typescript
private async sendMessage() {
  const text = this.inputText.trim();
  if (!text || !this.backendUrl || this.isLoading) return;

  if (!this.realSessionId) {
    // 创建 session
  }

  // 添加用户消息
  this.messages = [...this.messages, { id: `user-${Date.now()}`, role: 'user', content: text, timestamp: Date.now() }];
  this.inputText = '';
  this.isLoading = true;
  this.scrollToBottom();

  // 添加思考中占位符
  const loadingId = `loading-${Date.now()}`;
  this.messages = [...this.messages, { id: loadingId, role: 'assistant', content: '思考中...', timestamp: Date.now(), isLoading: true }];

  const url = `${this.backendUrl}/session/${encodeURIComponent(this.realSessionId)}/message`;
  const body = { parts: [{ type: 'text', text: text }] };

  try {
    const result = await new Promise((resolve, reject) => {
      this.currentRequest!.request(url, {
        method: http.RequestMethod.POST,
        header: this.viewModel.getHeaders(this.backendUrl, this.authToken, this.directory),
        extraData: JSON.stringify(body),
        connectTimeout: 120000,
        readTimeout: 120000,
      }, (err, data) => {
        if (err) reject(err);
        else resolve(data);
      });
    });

    this.messages = this.messages.filter(m => m.id !== loadingId);

    if (result.responseCode === 200) {
      const response = JSON.parse(result.result);
      const content = this.viewModel.formatMessage(response);
      this.messages = [...this.messages, {
        id: response.info?.id || `resp-${Date.now()}`,
        role: 'assistant',
        content: content || '完成',
        timestamp: Date.now()
      }];
    } else {
      this.messages = [...this.messages, {
        id: `error-${Date.now()}`,
        role: 'assistant',
        content: `请求失败: HTTP ${result.responseCode}`,
        timestamp: Date.now()
      }];
    }
  } catch (e) {
    this.messages = this.messages.filter(m => m.id !== loadingId);
    this.messages = [...this.messages, {
      id: `error-${Date.now()}`,
      role: 'assistant',
      content: `错误: ${e}`,
      timestamp: Date.now()
    }];
  } finally {
    this.isLoading = false;
    this.cancelRequest();
    this.scrollToBottom();
  }
}
```
