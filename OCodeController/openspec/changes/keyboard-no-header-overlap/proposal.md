## 为什么

当前对话页面唤起软键盘时，根 `Stack` 使用了 `expandSafeArea([SafeAreaType.KEYBOARD], [SafeAreaEdge.BOTTOM])`，导致整个页面（包括标题栏、消息列表、输入框）被统一向上推动。标题栏因此被推出了屏幕可见区域，影响用户体验。

## 改动内容

- 移除根 `Stack` 的 `SafeAreaType.KEYBOARD` 扩展，避免整个页面随键盘上移
- 保留 `SafeAreaType.KEYBOARD` 到输入框 `Row` 组件，仅让输入框区域随键盘上移
- 标题栏保持固定在屏幕顶部，不受键盘影响

## 能力

### 新增能力

- `keyboard-avoid-header`：在对话页面，软键盘唤起时仅将输入框区域向上推动，标题栏保持在屏幕顶部可见。

### 变更能力

- （无）

## 影响范围

- **修改的文件**：`entry/src/main/ets/pages/ChatPage.ets`
- **行为变更**：键盘弹出时页面不再整体上移，标题栏保持固定；输入框独立跟随键盘
