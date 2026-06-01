## 1. 状态和数据模型添加

- [ ] 1.1 向 `ChatPage` 添加 `@State historyCursor: string = ''`
- [ ] 1.2 向 `ChatPage` 添加 `@State hasMoreHistory: boolean = false`
- [ ] 1.3 向 `ChatPage` 添加 `@State isLoadingHistory: boolean = false`

## 2. 滚动到顶部检测

- [ ] 2.1 向 `Scroll` 组件添加 `onScrollStop` 或 `onScrollFrameBegin` 处理器，检测用户滚动到顶部
- [ ] 2.2 添加防抖守卫：仅在 `hasMoreHistory` 为 `true` 且 `isLoadingHistory` 为 `false` 时触发

## 3. 加载历史消息方法

- [ ] 3.1 向 `ChatPage` 添加 `private async loadPreviousMessages()` 方法，使用 `historyCursor` 和 `order='desc'` 调用 `viewModel.loadHistoryPage`
- [ ] 3.2 请求完成后将消息添加到 `this.messages` 头部
- [ ] 3.3 如果 `cursor.previous` 非空则更新 `historyCursor`，否则设置 `hasMoreHistory = false`
- [ ] 3.4 在 finally 块中设置 `isLoadingHistory = false`

## 4. 滚动位置维护

- [ ] 4.1 在添加消息前，通过 `this.scroller.currentOffset()` 记录当前滚动偏移量和内容高度
- [ ] 4.2 状态更新后，计算新增内容的高度差
- [ ] 4.3 调用 `this.scroller.scrollTo({ yOffset: offset + delta, duration: 0 })` 恢复位置

## 5. 加载指示器 UI

- [ ] 5.1 在 `Scroll` 的 `Column` 内部第一个子元素位置添加加载指示器（如 `LoadingProgress`），条件渲染 `isLoadingHistory` 为 `true` 时显示
- [ ] 5.2 确保指示器具有固定高度，避免出现/消失时布局跳动

## 6. 初始状态初始化

- [ ] 6.1 在 `loadHistory` 中初始 `loadHistoryPage` 完成后，如有 `page.cursor.previous` 则设置 `historyCursor`
- [ ] 6.2 根据 `cursor.previous` 是否有值设置 `hasMoreHistory`
- [ ] 6.3 在 `loadCachedMessages` 的缓存消息路径中也应用相同的初始化逻辑

## 7. "没有更多消息"提示

- [ ] 7.1 当 `hasMoreHistory` 为 `false` 且 `messages.length > 0` 且 `isLoadingHistory` 为 `false` 时，在顶部显示不显眼的"没有更多消息"文本
