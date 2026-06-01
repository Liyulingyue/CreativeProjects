## 背景

`SessionDetail.ets` 中的工作目录输入框已有基础的下拉提示功能，但触发逻辑存在问题：

```typescript
.onChange(v => {
  this.directory = v;
  if (this.showPathPicker) {  // 只在 picker 已显示时才搜索
    this.schedulePathSearch(v);
  }
})
.onFocus(() => {
  this.showPathPicker = true;
  this.schedulePathSearch(this.directory);
})
```

当前只有在 `onFocus` 时才会设置 `showPathPicker = true` 并触发搜索，`onChange` 时需要 `showPathPicker` 已经是 `true` 才会搜索。这意味着用户不先点击输入框就无法看到提示。

## 目标 / 非目标

**目标：**
- 输入框中输入文字时自动触发搜索和下拉提示
- 获得焦点时根据是否有内容决定显示"最近使用"还是搜索结果
- 保持 400ms 防抖，避免频繁请求后端

**非目标：**
- 不改变 `PathAutocomplete` 组件的 UI 样式
- 不修改 `SessionDetailViewModel` 的搜索算法逻辑
- 不支持多选或手动输入以外的其他交互

## 关键决策

### 1. 搜索触发时机

**决策**：在 `onChange` 中无条件触发搜索（不再依赖 `showPathPicker`）。

- 当用户输入文字时，立即设置 `showPathPicker = true` 并触发 `schedulePathSearch`。
- 保留 400ms 防抖，避免每敲一个字符都发请求。
- 如果输入为空，则调用 `listDirectory('/')` 显示根目录内容。

### 2. 焦点时的行为

**决策**：获得焦点时显示下拉列表，内容取决于当前输入框是否有内容。

- 如果输入框有内容：立即搜索该内容（复用已有逻辑）。
- 如果输入框为空：调用 `listDirectory('/')` 显示根目录的子目录列表，同时显示"最近使用"记录。

### 3. 列表显示优先级

**决策**：下拉列表中优先显示"最近使用"（最多5条），下方显示搜索/目录列表结果（最多8条）。

- 搜索进行中时显示 `LoadingProgress`。
- 搜索完成有结果时显示文件目录列表。
- 无结果时显示空状态。

## 风险 / 权衡

- **[风险] 每次输入都触发搜索**：用户输入过程中会不断触发防抖搜索。→ **缓解**：400ms 防抖已存在，网络请求有 cancel 机制。
- **[风险] 焦点时闪烁**：快速获得/失去焦点可能导致列表闪烁。→ **缓解**：200ms blur 延迟已存在，保持不变。
- **[风险] 输入为空时大量请求**：用户反复清空输入框可能产生额外请求。→ **缓解**：空输入调用 `listDirectory('/')`，后端 `/file` 接口本身支持分页。
