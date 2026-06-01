## 新增需求

### 需求：窗口创建时应用旋转锁定设置

系统在应用窗口创建时必须根据当前旋转锁定设置应用屏幕方向。

#### 场景：窗口创建且旋转已锁定
- **当** `EntryAbility.onWindowStageCreate` 被调用且 `rotationLocked` 为 `true` 时
- **则** 系统将屏幕方向设置为竖屏（`Orientation.PORTRAIT`）

#### 场景：窗口创建且旋转未锁定
- **当** `EntryAbility.onWindowStageCreate` 被调用且 `rotationLocked` 为 `false` 时
- **则** 系统将屏幕方向设置为跟随系统（`Orientation.AUTO`）

### 需求：设置变更时实时应用

系统在用户切换旋转锁定设置后必须实时应用屏幕方向变更，无需重启应用。

#### 场景：用户选择"锁定竖屏"
- **当** 用户点击"锁定竖屏"选项时
- **则** 系统将 `rotationLocked` 保存到本地存储
- **并且** 将 `AppStorage` 中的 `rotationLocked` 设置为 `true`
- **并且** 立即将屏幕方向设置为竖屏

#### 场景：用户选择"跟随系统"
- **当** 用户点击"跟随系统"选项时
- **则** 系统将 `rotationLocked` 保存到本地存储
- **并且** 将 `AppStorage` 中的 `rotationLocked` 设置为 `false`
- **并且** 立即将屏幕方向设置为跟随系统

### 需求：应用切换前台时重新应用设置

系统在应用从后台切换到前台时必须重新应用旋转锁定设置，以应对系统方向可能在应用后台时被改变的情况。

#### 场景：应用从后台切换到前台
- **当** `EntryAbility.onForeground` 被调用时
- **则** 系统读取当前 `rotationLocked` 设置
- **并且** 重新应用对应的屏幕方向
