## 背景

当前 `ChatPage.ets` 的 `build()` 方法结构如下：

```typescript
Stack() {                          // 根 Stack
  Column() {                       // 消息列表 + 输入框容器
    Scroll(...).layoutWeight(1)    // 消息列表
    Row() { ... }                 // 输入框 Row
  }
  Column() { ... }                // 标题栏，position({ y: 0 })
  .expandSafeArea([SYSTEM], [TOP]) // 仅扩展顶部状态栏区域
}
.expandSafeArea([SYSTEM, KEYBOARD], [TOP, BOTTOM])  // 问题所在
```

根 `Stack` 配置了 `expandSafeArea([SafeAreaType.KEYBOARD], [SafeAreaEdge.BOTTOM])`，使得整个页面（包括标题栏）随软键盘弹出向上推动。标题栏虽然独立配置了 `expandSafeArea([SYSTEM], [TOP])`，但仍在根 `Stack` 的键盘扩展区域内，导致被推出屏幕。

## 目标 / 非目标

**目标：**
- 软键盘弹出时，只有输入框区域被向上推动
- 标题栏始终固定在屏幕顶部，保持可见

**非目标：**
- 不改变页面布局结构
- 不修改键盘隐藏时的行为
- 不涉及其他页面的键盘行为

## 关键决策

### 1. 移除根 Stack 的键盘扩展

**决策**：将根 `Stack` 的 `expandSafeArea` 从 `[SYSTEM, KEYBOARD]` 改为仅 `[SYSTEM]`。

```typescript
// 修改前
.expandSafeArea([SafeAreaType.SYSTEM, SafeAreaType.KEYBOARD], [SafeAreaEdge.TOP, SafeAreaEdge.BOTTOM])

// 修改后
.expandSafeArea([SafeAreaType.SYSTEM], [SafeAreaEdge.TOP])
```

移除根 Stack 的键盘区域扩展后，键盘出现时不会再将整个页面推上屏幕。根 Stack 的高度保持在屏幕范围内，标题栏的 `position({ y: 0 })` 不再被影响。

### 2. 为输入框 Row 添加键盘扩展

**决策**：将 `SafeAreaType.KEYBOARD` 添加到输入框 `Row` 的 `expandSafeArea`。

```typescript
Row() { ... }
.width('100%')
.padding({ left: 8, right: 8, top: 8, bottom: 8 })
.backgroundColor(this.themeBgCard)
.expandSafeArea([SafeAreaType.KEYBOARD], [SafeAreaEdge.BOTTOM])
```

输入框 `Row` 通过独立配置键盘扩展，可以在键盘弹出时自动向上调整位置，确保输入框不被键盘遮挡。

## 风险 / 权衡

- **[风险] 输入框被键盘完全遮挡**：如果 `Row` 的键盘扩展不足以使输入框进入可见区域。→ **缓解**：HarmonyOS 的 `expandSafeArea` 会自动计算必要的偏移量，确保内容不被键盘遮挡。
- **[风险] 标题栏被键盘区域覆盖**：键盘弹出时，标题栏下方内容被推上，标题栏可能被屏幕顶部截断。→ **缓解**：根 Stack 不再扩展到键盘，标题栏始终在屏幕顶部可见。
