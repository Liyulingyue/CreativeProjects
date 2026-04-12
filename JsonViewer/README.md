# JSON 树形预览器

一个纯前端的 JSON 数据可视化工具，提供直观的树形结构展示。

## 📁 项目结构

```
json_viewer/
├── index.html          # 主入口文件
├── css/
│   └── styles.css      # 样式表 (CSS 变量、布局、组件样式、JSONL样式)
├── js/
│   ├── app.js          # 主应用模块 (初始化、事件绑定、协调、文件上传)
│   ├── tree-renderer.js # 树形渲染器 (递归渲染、语法高亮)
│   ├── sample-data.js  # 示例数据 (多组演示数据)
│   └── utils/
│       └── json-parser.js # JSONL 解析器 (JSON Lines 格式支持)
└── assets/             # 静态资源目录 (预留)
```

## 🏗️ 架构说明

### 模块化设计

| 模块 | 职责 |
|------|------|
| `app.js` | 应用入口，协调各模块，负责初始化和事件处理 |
| `tree-renderer.js` | 树形渲染引擎，将 JSON 递归渲染为 HTML |
| `sample-data.js` | 示例数据管理，提供多组演示数据 |
| `json-parser.js` | JSONL 解析器，支持 JSON Lines 格式 |

### 类图

```
JsonTreeViewerApp (主应用)
├── editor: JSONEditor (JSON 编辑器实例)
├── treeRenderer: TreeRenderer (树形渲染器实例)
├── init(): void (初始化应用)
├── handleJsonChange(): void (处理 JSON 变化)
├── toggleNode(id): void (切换节点状态)
├── expandAll(): void (展开所有)
├── collapseAll(): void (折叠所有)
└── loadSample(key): void (加载示例)

TreeRenderer (树形渲染器)
├── container: HTMLElement (渲染容器)
├── render(json): void (渲染整个树)
├── renderNode(node, key, isRoot): string
├── renderArray(arr, key, isRoot): string
├── renderObject(obj, key, isRoot): string
├── renderPrimitive(value, key, isRoot): string
├── expandAll(): void
├── collapseAll(): void
└── generateNodeId(): string

SampleData (示例数据)
├── default: object (默认示例)
├── nested: object (复杂嵌套示例)
├── arrays: object (数组示例)
├── getAllSamples(): array
└── getByKey(key): object
```

## 🚀 使用方法

### 方式一：本地服务器

```bash
cd workspace/Agent-03-planer/web_tools/json_viewer
python3 -m http.server 8080
# 访问 http://localhost:8080
```

### 方式二：直接打开

```bash
# 在浏览器中直接打开 index.html
```

## ✨ 功能特性

- 📝 **JSON 编辑** - 专业的代码编辑器，支持语法高亮
- 📤 **文件上传** - 支持上传 .json、.jsonl 或 .txt 文件（最大 5MB）
- 📄 **JSONL 支持** - 支持 JSON Lines 格式，每行独立 JSON 对象
- 🌳 **树形展示** - 递归渲染对象和数组
- 📂 **展开/折叠** - 点击节点或使用按钮控制
- 🎨 **语法高亮** - 字符串(红)、数字(蓝)、布尔(紫)、null(灰)
- ⚠️ **错误提示** - 非法 JSON 自动检测并显示错误
- 📋 **示例数据** - 提供多组示例数据快速体验
- ⌨️ **快捷键** - `Ctrl+Enter` 触发解析
- 📱 **响应式** - 适配移动端和桌面端

## 🎯 快捷键

| 快捷键 | 功能 |
|--------|------|
| `Ctrl/Cmd + Enter` | 解析 JSON |

## 📦 依赖

- **JSONEditor**: https://cdn.jsdelivr.net/npm/jsoneditor@9.10.2 (CDN)

无需安装任何本地依赖！

## 🔄 扩展指南

### 添加新的示例数据

编辑 `js/sample-data.js`:

```javascript
const SampleData = {
    // ... 现有数据
    
    myNewSample: {
        "自定义字段": "值"
    }
};
```

### 自定义样式

编辑 `css/styles.css`，修改 CSS 变量:

```css
:root {
    --color-string: #c41a16;  /* 修改字符串颜色 */
    --color-number: #1c00cf;  /* 修改数字颜色 */
    /* ... */
}
```

### 添加新的渲染类型

在 `js/tree-renderer.js` 中扩展 `renderNode` 方法:

```javascript
renderNode(node, key, isRoot = false) {
    // 添加自定义类型处理
    if (node instanceof Date) {
        return this.renderDate(node, key, isRoot);
    }
    // ... 原有逻辑
}
```

## 📝 许可证

MIT License
