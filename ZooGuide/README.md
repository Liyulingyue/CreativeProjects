# ZooGuide

> 南京红山森林动物园的「省力 Agent」 —— 帮你按自己的方式逛出趟只属于自己的红山。
> 后端 FastAPI + 前端 PWA（React + Vite + TypeScript）。

## 功能

- 🧭 **个性化路线规划**：基于时间预算、体力、是否带娃、是否怕晒、动物兴趣等，生成专属游园路线
- 💬 **叙事化讲解**：同一只长臂猿，给年轻人是"身手敏捷的社牛"，给带娃家长是"两岸猿声啼不住的主角"
- 🔄 **游中动态调整**：走累了？太阳晒？一键重新规划后半段
- 🦁 **动物打卡**：逛完积累成就，记录你的红山之旅

## 快速开始

### Backend

```bash
cd Backend
python3 -m venv .venv  # 已创建
source .venv/bin/activate
pip install -r requirements.txt

# 配置 LLM（可选，不配置则使用规则引擎回退）
cp .env.example .env
# 编辑 .env 填入 OPENAI_API_KEY

python run.py
# 访问 http://localhost:8000/docs
```

### Frontend

```bash
cd Web/PWA
npm install
npm run dev
# 访问 http://localhost:5173
```

## 项目结构

```
ZooGuide/
├── Backend/             FastAPI 后端
│   ├── app/             API 入口、路由、配置
│   ├── src/             核心库（planner、LLM、规则引擎、prompts）
│   ├── data/            静态 JSON（venues.json）
│   └── .venv/
├── Web/PWA/             React + Vite + TypeScript
│   ├── src/
│   └── public/
└── docs/                设计、API、数据文档
```

## 文档

- [`docs/design.md`](docs/design.md) - 产品设计文档
- [`docs/api.md`](docs/api.md) - API 契约
- [`docs/conventions.md`](docs/conventions.md) - 编码规范
- [`docs/data-sources.md`](docs/data-sources.md) - 数据来源

## 许可

MIT