# EnglishLearnHelper

英语学习辅助器

## 功能

- 📖 单词本 - 浏览和搜索雅思词汇
- 🎲 随机抽取 - 随机抽取单词学习
- ✍️ 短文生成 - 基于抽取的单词生成英语短文

## 快速开始

### 1. 克隆项目

```bash
git clone https://github.com/your-repo/EnglishLearnHelper.git
cd EnglishLearnHelper
```

### 2. 获取单词数据

```bash
# 创建 Data 目录并克隆单词库
mkdir Data
cd Data
git clone https://github.com/fanhongtao/IELTS.git
cd ..
```

### 3. 配置环境变量

```bash
cd backend
cp .env.example .env
# 编辑 .env，填入你的 API Key
```

### 4. 启动后端

```bash
cd backend
pip install -r requirements.txt
python run.py
```

后端运行在 http://localhost:8001

### 5. 启动前端

```bash
cd frontend
npm install
npm run dev
```

前端运行在 http://localhost:5174

## 技术栈

- 前端：React + TypeScript + Vite
- 后端：FastAPI + Python
- AI：OpenAI API (兼容)
