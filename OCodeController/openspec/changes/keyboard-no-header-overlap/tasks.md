## 1. 修改根 Stack 的 expandSafeArea

- [ ] 1.1 在 `ChatPage.ets` 的 `build()` 方法中，找到根 `Stack` 的 `.expandSafeArea([SafeAreaType.SYSTEM, SafeAreaType.KEYBOARD], [SafeAreaEdge.TOP, SafeAreaEdge.BOTTOM])`
- [ ] 1.2 将其修改为 `.expandSafeArea([SafeAreaType.SYSTEM], [SafeAreaEdge.TOP])`，移除 `SafeAreaType.KEYBOARD` 和 `SafeAreaEdge.BOTTOM`

## 2. 为输入框 Row 添加键盘扩展

- [ ] 2.1 找到 `build()` 方法中输入框所在的 `Row` 组件
- [ ] 2.2 在 `Row` 的 `.backgroundColor(this.themeBgCard)` 后添加 `.expandSafeArea([SafeAreaType.KEYBOARD], [SafeAreaEdge.BOTTOM])`

## 3. 验证标题栏固定

- [ ] 3.1 测试：唤起软键盘时，确认标题栏始终显示在屏幕顶部
- [ ] 3.2 测试：键盘隐藏后，确认标题栏仍在屏幕顶部

## 4. 验证输入框跟随键盘

- [ ] 4.1 测试：唤起软键盘时，确认输入框跟随键盘向上移动，不被键盘遮挡
- [ ] 4.2 测试：键盘隐藏后，确认输入框恢复到屏幕底部正常位置
