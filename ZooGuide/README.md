# ZooGuide

> 动物园省力 Agent —— 帮你按自己的方式逛出趟只属于自己的路线。
> 后端 FastAPI + 前端 PWA（React + Vite + TypeScript）。

## 核心特性

- **全配置化**：换动物园只需改 `Backend/data/venues.json` + `Backend/data/system.json` + `Backend/data/downloads/`，前后端代码无需修改
- 🧭 **个性化路线规划**：基于时间预算、体力、是否带娃、是否怕晒、动物兴趣等，生成专属游园路线
- 💬 **叙事化讲解**：同一只长臂猿，给年轻人是"身手敏捷的社牛"，给带娃家长是"两岸猿声啼不住的主角"
- 🔄 **游中动态调整**：走累了？太阳晒？一键重新规划后半段
- 🦁 **动物打卡**：逛完积累成就，记录旅程
- 📸 **照片点评**：AI 识别动物 + 幽默点评 + 出片徽章
- 📍 **GPS 打卡**：定位最近场馆，一键签到

## 快速开始

### Backend

```bash
cd Backend
python3 -m venv .venv
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

### 测试

```bash
cd Backend
source .venv/bin/activate
python -m pytest tests/ -v
```

## 项目结构

```
ZooGuide/
├── Backend/                 FastAPI 后端
│   ├── app/                 API 入口、路由、配置
│   │   ├── main.py          路由定义（28 个端点）
│   │   ├── planner.py       路线规划（规则引擎 + LLM）
│   │   ├── chat.py          Agent 对话（工具调用）
│   │   ├── rule_engine.py   硬约束过滤 + 评分
│   │   ├── walking.py       步行距离矩阵
│   │   ├── photo.py         照片点评
│   │   ├── geo.py           GPS 距离计算
│   │   ├── db.py            SQLite 持久化
│   │   └── ...
│   ├── data/                配置与数据
│   │   ├── venues.json      场馆 + meta（唯一真相源）
│   │   ├── system.json      Agent 身份 + 规则 + 梗
│   │   └── downloads/       访客可下载的 PDF
│   ├── tests/               单元测试
│   └── .venv/
├── Web/PWA/                 React + Vite + TypeScript
│   ├── src/
│   └── public/
├── docs/                    设计、API、数据文档
└── e2e_test.py              端到端冒烟测试
```

## 配置化说明

所有动物园特有内容集中在 `Backend/data/venues.json` 的 `meta` 字段：

| 字段 | 用途 |
|------|------|
| `name` / `short_name` | 园区名称，全局使用 |
| `gates` | 入口位置（GPS + 标签 + 描述） |
| `areas` / `area_icons` | 片区划分及图标 |
| `venue_emojis` | 场馆 emoji 映射 |
| `bbox` | 园区坐标边界框 |
| `walking` | 步行参数（路径倍率、速度） |
| `interest_map` | 兴趣标签 → 场馆匹配规则 |
| `entity_map` | 中文名 → venue_id 映射 |
| `chat_defaults` | 对话默认语（问候语等） |
| `planner_defaults` | 路线规划模板 |
| `achievements` | 成就定义 |
| `warnings` | 通用警告 |
| `prompt_extras` | LLM 提示词模板 |
| `photo_venue_captions` | 照片点评配文 |

`system.json` 中的 `{name}` / `{short_name}` 会在运行时自动替换。

## 文档

- [`docs/design.md`](docs/design.md) - 产品设计文档
- [`docs/api.md`](docs/api.md) - API 契约
- [`docs/conventions.md`](docs/conventions.md) - 编码规范
- [`docs/data-sources.md`](docs/data-sources.md) - 数据来源

## 许可

MIT
