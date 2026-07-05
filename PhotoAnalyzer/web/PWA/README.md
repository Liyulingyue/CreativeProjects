# PhotoAnalyzer PWA

照片分析快捷辅助工具 - 纯前端 PWA 应用

## 定位

- **快捷辅助工具**：帮助你快速判断照片的画面质量、风格、内容等
- **完全本地运行**：图片仅在浏览器本地处理，不上传任何服务器
- **无需后端**：纯前端应用，不依赖任何后端服务
- **离线可用**：首次加载后支持离线使用

## 特性

- 单张或批量分析照片
- 多维度评分：质量、清晰度、风格、构图等
- PWA 安装：可添加到手机桌面
- 离线支持：缓存后无网也能用
- 主题切换：支持浅色/深色模式
- 完全隐私：图片不离开你的设备

## 技术栈

- React 18
- TypeScript
- Vite
- Vite PWA

## 开发

```bash
cd web/PWA
npm install
npm run dev
```

## 构建

```bash
npm run build
npm run preview
```

## 隐私说明

所有图片处理均在浏览器本地完成，通过 Web API 直接调用 AI 模型接口（OpenAI Compatible API），图片数据不会经过任何第三方服务器。

## API 配置

需要自行配置 OpenAI 兼容 API 的：
- API Key
- Base URL
- 模型名称

配置会在浏览器本地保存。
