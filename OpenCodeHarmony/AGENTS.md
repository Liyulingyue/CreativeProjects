# OpenCode Harmony 开发规则指南

本文档总结了 ArkTS/HarmonyOS 开发中的常见问题和最佳实践，供 AI 助手参考。

## 一、类型系统规则

### 1.1 对象字面量必须显式类型声明

**问题**: ArkTS 不支持无类型的对象字面量 (arkts-no-untyped-obj-literals)

**错误示例**:
```typescript
// ❌ 错误：缺少显式类型
this.sessions = backendSessions.map(s => ({
  id: s.id,
  name: s.title
}));
```

**正确做法**:
```typescript
// ✅ 定义接口
interface SessionItem {
  id: string;
  name: string;
}

// ✅ 添加显式返回类型
this.sessions = backendSessions.map((s): SessionItem => ({
  id: s.id,
  name: s.title
}));
```

### 1.2 禁止对象字面量作为类型声明

**问题**: 对象字面量不能用于类型声明 (arkts-no-obj-literals-as-types)

**错误示例**:
```typescript
// ❌ 错误：内联对象类型
const state = part.state as { status?: string; title?: string } | undefined;
```

**正确做法**:
```typescript
// ✅ 定义接口
interface ToolState {
  status?: string;
  title?: string;
}

const state = part.state as ToolState | undefined;
```

### 1.3 禁止 any 和 unknown 类型

**问题**: 必须使用显式类型 (arkts-no-any-unknown)

**错误示例**:
```typescript
// ❌ 错误：JSON.parse 返回 any
const session = JSON.parse(result.result as string);
```

**正确做法**:
```typescript
// ✅ 定义接口并断言类型
interface SessionResponse {
  id: string;
}

const session = JSON.parse(result.result as string) as SessionResponse;
```

### 1.4 禁止 as const 断言

**问题**: ArkTS 不支持 `as const` 断言 (arkts-no-as-const)

**错误示例**:
```typescript
// ❌ 错误
type: 'directory' as const
```

**正确做法**:
```typescript
// ✅ 通过接口类型约束
interface FileNode {
  type: 'file' | 'directory';
}

const node: FileNode = {
  type: 'directory'  // 自动推断为合法值
};
```

### 1.5 数组字面量元素类型必须可推断

**问题**: 数组元素类型必须可推断 (arkts-no-noninferrable-arr-literals)

**错误示例**:
```typescript
// ❌ 错误：对象字面量类型不可推断
const body = {
  parts: [{ type: 'text', text }]
};
```

**正确做法**:
```typescript
// ✅ 定义接口
interface TextPart {
  type: 'text';
  text: string;
}

interface MessageBody {
  parts: TextPart[];
}

const body: MessageBody = {
  parts: [{ type: 'text', text: text }]
};
```

### 1.6 操作符优先级必须明确

**问题**: `||` 和 `??` 混用时需要括号明确优先级

**错误示例**:
```typescript
// ❌ 错误：优先级不明确
const name = this.project?.notes || this.project?.name ?? '未知';
```

**正确做法**:
```typescript
// ✅ 使用括号明确优先级
const name = (this.project?.notes || this.project?.name) ?? '未知';
```

## 二、组件 API 规则

### 2.1 使用正确的组件属性

**问题**: 某些属性不存在于特定组件

**错误示例**:
```typescript
// ❌ 错误：Column 没有 maxHeight 属性
Column() {
  // ...
}
.maxHeight(300)
```

**正确做法**:
```typescript
// ✅ 使用 constraintSize 限制尺寸
Column() {
  // ...
}
.constraintSize({ maxHeight: 300 })
```

### 2.2 Web 组件 API 差异

**注意**: HarmonyOS Web 组件 API 与标准 Web 不同

- 不存在 `onWebResourceRequest`
- 认证头应通过 `onPageBegin` 注入或 URL 参数传递
- 参考 HarmonyOS 官方文档确认有效 API

## 三、最佳实践

### 3.1 接口定义规范

```typescript
// 推荐：集中定义接口在文件顶部
interface DisplayMessage {
  id: string;
  role: 'user' | 'assistant';
  content: string;
  timestamp: number;
  isLoading?: boolean;  // 可选属性放最后
}
```

### 3.2 类型断言规范

```typescript
// ✅ 推荐：router 参数处理
const params = router.getParams() as Record<string, Object>;
if (params) {
  this.sessionId = (params['sessionId'] as string) || '';
}
```

### 3.3 HTTP 请求规范

```typescript
// ✅ 推荐：定义请求体接口
interface RequestBody {
  parts: Part[];
}

const body: RequestBody = {
  parts: [{ type: 'text', text: input }]
};

// 使用 JSON.stringify 序列化
extraData: JSON.stringify(body)
```

## 四、常见错误速查表

| 错误代码 | 错误名称 | 解决方案 |
|---------|---------|---------|
| 10605038 | arkts-no-untyped-obj-literals | 添加显式类型声明 |
| 10605040 | arkts-no-obj-literals-as-types | 定义接口替代内联类型 |
| 10605008 | arkts-no-any-unknown | 使用显式类型断言 |
| 10605142 | arkts-no-as-const | 移除 as const，用接口约束 |
| 10605043 | arkts-no-noninferrable-arr-literals | 定义数组元素接口 |
| 10505001 | Property not exist | 查阅文档使用正确 API |

## 五、开发工作流

1. **编译前检查**: 使用 `builtin_check_editor_errors` 检查语法
2. **编译验证**: 使用 `builtin_execute_build_command` 验证编译
3. **查阅文档**: 使用 `builtin_web_rag` 查询 HarmonyOS 官方文档
4. **错误修复**: 按本指南规则逐一修复类型错误

---

*本文档基于实际开发经验总结，持续更新中。*
