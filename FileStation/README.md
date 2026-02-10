# FileStation

FileStation 是一个极简、高性能且面向未来的现代文件管理系统。它专注于提供稳健的文件存储底座，并为后续的 AI 向量化检索、多版本追踪及自动化标注预留了无限的插件化扩展能力。

##  核心理念

**Simple, Fast, and AI-Ready.**
FileStation 不仅仅是一个网盘，它是你的个人知识仓库。我们剔除了所有复杂的概念，只保留最核心、最直观的文件操作体验。

##  主要功能

-  **极简文件管理**：支持秒级的上传、下载、删除及跨文件夹拖拽移动，操作逻辑与原生操作系统一致。
-  **内容寻址存储**：底层采用 SHA256 内容指纹技术，天然支持文件去重（秒传），节省存储空间。
-  **多模态展示**：支持网格视图（预览风格）与列表视图（详细信息），适应不同管理习惯。
-  **插件化扩展 (Roadmap)**：
    - **History Plugin**：基于内容指纹的版本回溯能力（现已内置核心）。
    - **MetaParser Plugin**：自动提取 Office、PDF 及图片的元数据，实现自动分类。

##  技术架构

本项目坚持**极简主义**架构逻辑，确保系统轻量且易于部署。

| 模块 | 技术选型 | 优势 |
| :--- | :--- | :--- |
| **后端 (Backend)** | FastAPI | 异步高性能，极速构建 REST 接口。 |
| **前端 (Frontend)** | React + TailwindCSS | 响应式设计，极致的桌面级 Web 交互。 |
| **数据库** | SQLite | 零配置，单文件持久化。 |
| **存储策略** | Content-Addressed Local Storage | 像 Git 一样管理 Blob，但对用户完全透明。 |

##  快速开始

### 后端启动
```bash
cd backend
pip install -r requirements.txt
python run.py
```

### 前端启动
```bash
cd frontend
npm install
npm run dev
```

---

##  迭代记录

- **v0.3.0**: 品牌重构为 FileStation，强化底座能力，支持跨文件夹拖拽移动及多视图切换。
- **v0.2.0**: 实现基础的历史记录对比功能。
- **v0.1.0**: MVP 诞生，支持基础上传下载。
