## 为什么

当前系统设置中有屏幕旋转锁定功能，用户可以在"跟随系统"和"锁定竖屏"之间切换，但切换后屏幕方向并不会被锁定。原因是旋转锁定的设置值虽然被保存到了本地，但 HarmonyOS 原生端（`EntryAbility`）和 Android 端（`EntryEntryAbilityActivity`）都没有正确读取和应用该设置。

## 改动内容

- 在 `EntryAbility` 中使用 HarmonyOS `@kit.ArkUI` 的 `window` API，根据 `rotationLocked` 设置在窗口创建时设置屏幕方向
- 在用户切换设置时，实时应用屏幕方向变更
- 移除 Android Activity 中不可靠的反射读取方式，改为使用 ArkUI-X 提供的标准接口

## 能力

### 新增能力

- `screen-rotation-lock`：在 HarmonyOS 设备上，根据用户设置的"跟随系统"或"锁定竖屏"选项，实际控制应用的屏幕方向。

### 变更能力

- （无）

## 影响范围

- **修改的文件**：
  - `entry/src/main/ets/entryability/EntryAbility.ets` — 应用窗口方向控制
  - `entry/src/main/ets/components/AboutPage.ets` — 切换设置时实时应用方向
  - `.arkui-x/android/.../EntryEntryAbilityActivity.java` — 改进 Android 端设置读取方式
