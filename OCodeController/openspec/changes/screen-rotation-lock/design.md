## 背景

当前旋转锁定的实现存在两部分问题：

### HarmonyOS 端（`EntryAbility.ets`）

`EntryAbility` 已导入 `window` from `@kit.ArkUI`，但从未使用它来控制屏幕方向。`EntryAbility.onWindowStageCreate` 创建窗口时没有读取 `rotationLocked` 设置，导致无论用户选择什么，屏幕方向都由系统决定。

```typescript
// EntryAbility.ets - 当前 onWindowStageCreate
onWindowStageCreate(windowStage: window.WindowStage): void {
  // 没有读取或应用 rotationLocked 设置
  windowStage.loadContent('pages/MainPage', ...);
}
```

### Android 端（`EntryEntryAbilityActivity.java`）

`applyRotationLock()` 方法通过反射访问 `AppStorage`，这种方式的可行性不稳定：
- 反射调用 `AppStorage.getInstance()` 和 `getBool("rotationLocked", false)` 可能在不同版本中失败
- ArkUI-X 的 `AppStorage` 可能尚未初始化，导致读取到默认值

## 目标 / 非目标

**目标：**
- 在 HarmonyOS 设备上正确实现屏幕竖屏锁定
- 用户切换设置后实时生效（无需重启应用）
- 在窗口创建时正确应用锁定设置

**非目标：**
- 不实现横屏锁定（仅支持竖屏锁定和跟随系统）
- 不改变 Android/iOS ArkUI-X 端的实现（保持现状）

## 关键决策

### 1. HarmonyOS 端：使用 window API 设置方向

**决策**：在 `EntryAbility.onWindowStageCreate` 中获取窗口并设置方向。

关键 API：
```typescript
window.getLastWindow(this.context, (err, win) => {
  if (isLocked) {
    win.setPreferredOrientation(window.Orientation.PORTRAIT);
  } else {
    win.setPreferredOrientation(window.Orientation.AUTO);
  }
});
```

- `Orientation.PORTRAIT`：锁定竖屏
- `Orientation.AUTO`：跟随系统重力感应

### 2. 设置变更时实时应用

**决策**：在 `AboutPage.ets` 的设置切换回调中直接调用 `window` API 应用方向变更。

```typescript
.onClick(() => {
  this.isRotationLocked = true;
  this.core.setRotationLocked(true);
  AppStorage.setOrCreate<boolean>('rotationLocked', true);
  // 立即应用方向
  this.applyRotationLock(true);
})
```

需要抽取一个 `applyRotationLock` 方法，在 `AboutPage` 中通过 `EntryAbility` 的上下文或共享工具调用。

### 3. 抽取通用方向设置工具

**决策**：在 `OpenCodeCore` 中添加一个静态方法 `applyScreenOrientation`，供 `EntryAbility` 和 `AboutPage` 共用。

```typescript
public static async applyScreenOrientation(context: Context, isLocked: boolean): Promise<void> {
  const win = await window.getLastWindow(context);
  if (isLocked) {
    win.setPreferredOrientation(window.Orientation.PORTRAIT);
  } else {
    win.setPreferredOrientation(window.Orientation.AUTO);
  }
}
```

## 风险 / 权衡

- **[风险] `getLastWindow` 异步失败**：在窗口未就绪时调用可能失败。→ **缓解**：在 `onWindowStageCreate` 中调用，此时窗口已创建。
- **[风险] 平板设备方向**：平板通常默认横屏，锁定竖屏可能体验不佳。→ **缓解**：仅在"锁定竖屏"选项下生效，平板用户通常选择"跟随系统"。
- **[风险] 权限问题**：某些设备可能限制应用设置方向。→ **缓解**：`setPreferredOrientation` 是标准 API，大多数设备支持。
