## 背景

聊天页面（`ChatPage.ets`）在页面加载时通过 `ChatViewModel.loadHistoryPage` 加载最新的 20 条消息，使用 `Scroll` 组件配合 `ForEach` 渲染消息。一旦加载完成，没有机制再加载更早的消息。

现有的 `loadHistoryPage` API 支持基于游标的分页，传入 `order='desc'` 并返回 `cursor.previous` 字段。这是本次改动的基础。

## 目标 / 非目标

**目标：**
- 支持用户滚动到消息列表顶部时加载更早的消息
- 在添加历史消息后保持滚动位置不变（避免跳动）
- 在加载中和无更多历史时提供视觉反馈

**非目标：**
- 不预加载超出用户请求范围的消息
- 不支持加载更新的消息（已在底部，最新的消息）
- 不改变默认加载 20 条的初始行为

## 关键决策

### 1. 滚动到顶部检测方案

**决策**：使用 `onScrollStop` + 滚动偏移量阈值检测。

- 当滚动偏移量接近 0（顶部）时，触发历史消息加载。
- 通过 `onScrollFrameBegin` 中的 `scrollOffset` 跟踪是否在顶部。
- 也可以检测第一个可见项何时变为第一条消息。

**考虑的替代方案：**
- 下拉刷新（`Refresh` 组件）：HarmonyOS 的 `Refresh` 设计用于 `List` 的顶层刷新，不适合配合 `Scroll` 和 `ForEach` 使用。
- 自定义滑动手势：更复杂，不够自然。
- 滚动阈值：简单可靠，与现有模式一致。

### 2. 添加消息后的滚动位置维护

**决策**：在添加消息前记录当前滚动高度，状态更新后恢复位置。

- 在加载历史消息前，记录 `this.scroller.currentOffset().yOffset` 和当前滚动内容高度。
- 在消息被添加到前面、`LazyForEach` 重新渲染后，计算新增内容的高度差。
- 使用 `scroller.scrollTo({ yOffset: offset + delta, duration: 0 })` 恢复位置。
- 对滚动到顶部的检测添加短暂防抖，避免在同一次滚动中触发多次。

### 3. 状态字段添加

**决策**：向 `ChatPage` 添加三个 `@State` 字段：
- `hasMoreHistory: boolean` — 是否有更早的消息可加载（由 `cursor.previous` 决定）
- `isLoadingHistory: boolean` — 是否正在加载历史消息
- `historyCursor: string` — 下一条历史消息页的 `cursor.previous` 值

### 4. API 集成

**决策**：加载更多时调用 `viewModel.loadHistoryPage(..., historyCursor, 20, 'desc')`。

- 复用现有 `loadHistoryPage`，传入 `cursor` 为 `historyCursor`。
- 如果 `cursor.previous` 为空/空，则设置 `hasMoreHistory = false`。
- 用 `isLoadingHistory` 防止并发加载。

### 5. 加载指示器放置

**决策**：当 `isLoadingHistory` 为 true 时，在第一条消息上方显示细长的进度指示器。

- 将小型加载指示器（Text 或 LoadingProgress）作为 Scroll 内 Column 的第一个子元素。
- 使用固定高度的指示器，避免出现/消失时对现有消息造成意外位移。

## 风险 / 权衡

- **[风险] 添加消息后滚动跳动**：`Scroll` + `ForEach` 中在头部添加元素如果不妥善处理会导致视图跳动。→ **缓解**：按决策 2 所述测量并恢复偏移量。
- **[风险] 快速多次滚动触发**：用户可能在顶部来回滚动触发多次加载。→ **缓解**：用 `isLoadingHistory` 守卫防抖；当 `hasMoreHistory` 为 false 时跳过。
- **[风险] 服务器无更多历史**：`cursor.previous` 可能为空，或 API 返回空数组。→ **缓解**：设置 `hasMoreHistory = false`，加载完所有消息后显示"没有更多消息"提示。

## 迁移计划

1. 向 `ChatPage` 添加状态字段
2. 添加滚动帧回调检测滚动到顶部
3. 添加 `loadPreviousMessages` 方法
4. 在消息列表顶部添加加载指示器 UI
5. 从初始 `MessagePage` 结果中设置 `hasMoreHistory` 和 `historyCursor`
6. 测试滚动到顶部行为和滚动位置维护
