## 1. 修改 onChange 逻辑

- [ ] 1.1 在 `SessionDetail.ets` 的 `buildDirectorySection()` 中，修改 `TextInput` 的 `onChange` 回调：移除 `if (this.showPathPicker)` 条件，改为无条件设置 `this.showPathPicker = true` 并调用 `this.schedulePathSearch(v)`
- [ ] 1.2 处理空输入情况：当 `v` 为空时调用 `listDirectory('/')` 并显示根目录内容

## 2. 验证 onFocus 逻辑

- [ ] 2.1 确认 `onFocus` 回调在输入框获得焦点时设置 `this.showPathPicker = true` 并调用 `schedulePathSearch`
- [ ] 2.2 如果 `directory` 为空，确保 `onFocus` 调用 `listDirectory('/')` 而非空搜索

## 3. 验证下拉列表显示逻辑

- [ ] 3.1 确认 `PathAutocomplete` 在 `showPathPicker` 为 `true` 时正确显示
- [ ] 3.2 确认"最近使用"列表在有记录时始终显示在顶部
- [ ] 3.3 确认搜索进行中显示 `LoadingProgress`，搜索完成显示结果列表

## 4. 验证列表项交互

- [ ] 4.1 确认点击目录项后 `onPathSelect` 正确设置 `directory` 并收起列表
- [ ] 4.2 确认点击最近使用路径后 `onRecentPathSelect` 正确设置 `directory` 并收起列表

## 5. 回归测试

- [ ] 5.1 测试从会话列表新建会话流程，确认目录输入框行为符合预期
- [ ] 5.2 测试输入任意路径片段，确认下拉提示正常显示
- [ ] 5.3 测试选择目录项后路径正确填充
- [ ] 5.4 测试无后端连接时输入框仍可手动输入（不影响降级体验）
