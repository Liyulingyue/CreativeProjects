# MiniCoder Web - 前端界面

基于 React + TypeScript 的 MiniCoder Web 前端界面。

## 🚀 快速开始

```bash
# 安装依赖
npm install

# 启动开发服务器
npm start

# 构建生产版本
npm run build
```

## 📁 项目结构

```
mini-coder-web/
├── src/
│   ├── components/       # React组件
│   │   ├── CodeEditor.tsx
│   │   └── CodeEditor.css
│   ├── services/        # API服务
│   │   └── api.ts
│   ├── types/           # TypeScript类型定义
│   │   └── index.ts
│   ├── App.tsx          # 主应用组件
│   ├── App.css          # 主应用样式
│   ├── index.tsx        # 应用入口
│   └── index.css        # 全局样式
├── public/              # 静态资源
├── package.json         # 依赖配置
└── tsconfig.json        # TypeScript配置
```

## ✨ 功能特性

- 🎨 现代化UI设计，渐变背景和卡片式布局
- 💻 代码编辑器组件，支持语法高亮（基础样式）
- 🔧 四种核心功能：
  - 生成代码
  - 解释代码
  - 修复bug
  - 优化代码
- 📱 响应式设计，支持移动端
- 🌐 TypeScript类型安全

## 🔌 API集成

前端通过 `MiniCoderApi` 类与后端交互：

```typescript
// 生成代码
MiniCoderApi.generateCode(prompt, language)

// 解释代码
MiniCoderApi.explainCode(code)

// 修复bug
MiniCoderApi.fixBug(errorMessage, codeContext)

// 优化代码
MiniCoderApi.optimizeCode(code)
```

## 🎨 UI组件

### CodeEditor
可复用的代码编辑器组件，支持：
- 只读模式
- 自定义占位符
- Monospace字体
- 深色主题

## 🛠️ 技术栈

- React 18
- TypeScript
- CSS3（Flexbox、Grid、渐变）
- Fetch API

## 📝 待办事项

- [ ] 添加语法高亮库（如react-syntax-highlighter）
- [ ] 实现代码复制功能
- [ ] 添加加载动画
- [ ] 集成后端API（需要先启动MiniCoder后端）
- [ ] 添加错误处理和重试机制
- [ ] 实现代码下载功能

## 💡 使用说明

1. 启动后端MiniCoder服务
2. 启动前端开发服务器：`npm start`
3. 访问 http://localhost:3000
4. 选择功能并输入代码/描述
5. 点击执行按钮查看结果

## 🔧 配置

API地址配置在 `.env` 文件中：

```env
REACT_APP_API_URL=http://localhost:8000
```

---
**创建时间**: 2026-02-18
**技术栈**: React + TypeScript
