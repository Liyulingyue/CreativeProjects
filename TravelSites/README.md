# TravelSites — 时空驱动的旅游目的地发现平台

> 你不再需要先决定"去哪"，AI 替你穷举出"在你假期长度内、车程可承受、游玩体量匹配"的所有目的地。

## 快速开始

### 1. 安装依赖

```bash
# Python 3.12+
source .venv/bin/activate  # 或创建: python -m venv .venv
pip install -r requirements.txt

# Node 18+（前端构建）
cd web && npm install && npm run build
```

### 2. 配置环境变量

复制 `.env.example` 为 `.env` 并填入：

```bash
cp .env.example .env
# 编辑 .env，至少配置 OPENAI_API_KEY
```

### 3. 启动服务

```bash
python run.py
# 访问 http://localhost:8000
```

首次启动会自动：
- 初始化 SQLite 事实数据库（`data/travelsites.db`）
- 从 `.env` 读取种子城市并写入 DB
- 创建默认管理员账户（`admin / admin123`）

## 架构

```
┌─────────────────────────────────────────────────────────┐
│  Frontend (React + Vite + PWA)                          │
│  - 首页/搜索/详情/管理后台                                │
│  - LocalStorage 存 token                                  │
└────────────────────┬────────────────────────────────────┘
                     │ HTTP + Bearer token
┌────────────────────┴────────────────────────────────────┐
│  Backend (FastAPI)                                     │
│  /api/search      公开 — 出行搜索                          │
│  /api/auth/*      公开 — 注册/登录/登出/me              │
│  /api/cities      公开 — 城市列表                         │
│  /api/holidays    公开 — 节假日洞察                       │
│  /api/admin/*     需 admin — 城市管理、刷新触发            │
└────────────────────┬────────────────────────────────────┘
                     │
┌────────────────────┴────────────────────────────────────┐
│  SQLite (data/travelsites.db)                           │
│  - geo_provinces / geo_cities / geo_counties             │
│  - holiday_calendar（节假日 + 扩展字段 region/demographic）│
│  - trip_matrix_cache（预生成方案）                        │
│  - seed_config（可运行时改的城市列表）                   │
│  - users / user_sessions（用户与认证）                   │
│  - generation_log（生成历史与统计）                     │
└─────────────────────────────────────────────────────────┘
                     │
                     ↓（定时/手动触发）
┌─────────────────────────────────────────────────────────┐
│  SQLite (data/travelsites.db)                           │
│  - geo_provinces / geo_cities / geo_counties             │
│  - holiday_calendar（节假日 + 扩展字段 region/demographic）│
│  - attractions（真实景点库，35 城市 134 景点）            │
│  - trip_matrix_cache（预生成方案）                        │
│  - seed_config（可运行时改的城市列表）                   │
│  - users / user_sessions（用户与认证）                   │
│  - generation_log（生成历史与统计）                     │
└─────────────────────────────────────────────────────────┘
                     │
                     ↓（定时/手动触发）
┌─────────────────────────────────────────────────────────┐
│  Matrix Generation (LLM)                                 │
│  src/planner.py — ReAct 生成完整行程                       │
│  src/matrix.py  — 多 cell 并发（MATRIX_CONCURRENCY）       │
│  生成结果写回 SQLite + JSON 备份                          │
└─────────────────────────────────────────────────────────┘
                     ↓
              search 时优先用 attractions 表覆盖 LLM 幻觉
```

## 核心概念

### Matrix（预生成矩阵）

每个城市 × 多个 (出发日偏移, 出行天数) 组合预生成行程方案：
- 例：济南 + (offset=1, duration=2) = 济南 "明天出发、玩 2 天" 的方案
- 全量生成：35 城市 × 2 cells = 70 次 LLM 调用（lite=False）
- 写入 `trip_matrix_cache` 表

### Search（运行时检索）

用户查询时：
1. 按日期范围从 DB 查出匹配的 cell
2. 用 Haversine + 交通估算重新算 `transport_score`
3. 综合 4 维评分：天数 40% + 车程 25% + 天气 25% + 景点 10%
4. 按 score 排序返回

### Holidays（节假日洞察）

节假日不调整推荐分数（用户已选日期），只输出：
- `crowd_level` 人流密度
- `activity_level` 活动丰富度
- `price_multiplier` 价格上浮
- `tips` 出行提示文案

未来可扩展：
- 地区差异（HK/MO/TIB）
- 人群差异（学生/上班族）

## 配置项（.env）

| 变量 | 说明 | 默认 |
|------|------|------|
| `OPENAI_API_KEY` | LLM API key | — |
| `OPENAI_BASE_URL` | LLM 端点 | minimaxi |
| `OPENAI_VISION_MODEL_NAME` | 模型名 | MiniMax-M3 |
| `REFRESH_ENABLED` | 是否启用定时刷新 | false |
| `REFRESH_INTERVAL_SECONDS` | 刷新间隔 | 3600 |
| `MATRIX_MAX_OFFSET` | 未来出发日天数 | 1 |
| `MATRIX_MAX_DURATION` | 出行天数 | 2 |
| `MATRIX_CONCURRENCY` | LLM 并发数 | 3 |
| `SEED_CITIES` | 种子城市（逗号分隔）| 35 城 |
| `ADMIN_USERNAME` | 默认管理员账号 | admin |
| `ADMIN_PASSWORD` | 默认管理员密码 | admin123 |
| `SESSION_DAYS` | Session 有效期 | 30 |

## API 文档

### 公开接口

```
POST /api/search
  body: {start_date, end_date, preference?, origin_province, origin_city, origin_county}
  resp: {items: [{city, score, distance_km, transport_mode, ...}], total}

GET  /api/cities
GET  /api/cities/{city}         → 该城市 matrix
GET  /api/holidays?start_date=&end_date=
POST /api/refresh              → 触发全量刷新（生产慎用）
GET  /api/refresh/status
GET  /api/health                → 系统概览
```

### 认证接口

```
POST /api/auth/register   body: {username, password, email?, display_name?}
POST /api/auth/login      body: {username, password} → {token, expires_at, user}
POST /api/auth/logout     (需 Bearer token)
GET  /api/auth/me          (需 Bearer token)
```

### 管理员接口（需 admin role）

```
GET  /api/admin/overview             系统统计
GET  /api/admin/cities               当前 seed cities
PUT  /api/admin/cities               body: {cities: [...]}
POST /api/admin/cities/{city}/refresh  触发某城市重新生成
GET  /api/admin/logs?limit=20         最近生成日志
```

## 常用命令

```bash
# 初始化数据库（首次启动自动）
python -c "from src.db import init_db, init_seed_cities; init_db(); init_seed_cities()"

# 从 JSON 迁移 matrix cache 到 DB
python src/db_migrate_matrix.py

# 修复城市坐标
python src/db_fix_coords.py

# 填充节假日数据
python src/holidays.py

# 填充景点种子数据（35 城市 134 景点）
python src/db_seed_attractions.py

# 手动触发某城市重新生成
curl -X POST http://localhost:8000/api/admin/cities/济南/refresh \
  -H "Authorization: Bearer <admin_token>"

# 查看系统健康
curl http://localhost:8000/api/health

# 查询某城市景点
curl "http://localhost:8000/api/attractions?city=杭州"
```

## 目录结构

```
TravelSites/
├── app/                       FastAPI 后端
│   ├── main.py               应用入口 + lifespan
│   ├── config.py             配置加载
│   ├── deps.py               认证依赖注入
│   ├── router.py             所有 API 路由
│   ├── refresh.py            定时/手动刷新逻辑
│   ├── matrix.py             matrix 生成（备份于 src/matrix.py）
│   ├── search_models.py      Pydantic 模型
│   └── models.py             通用响应模型
├── src/                       核心库
│   ├── db.py                 SQLite schema + 查询 API
│   ├── db_fix_coords.py      坐标补全脚本
│   ├── db_migrate_matrix.py  JSON → SQLite 迁移
│   ├── auth.py               认证（bcrypt + token）
│   ├── holidays.py           节假日数据 + 洞察
│   ├── distance.py           Haversine + transport_score
│   ├── cities.py             城市查找（运行时学习）
│   ├── weather.py            Open-Meteo 集成
│   ├── planner.py            LLM ReAct 生成器
│   ├── matrix.py             matrix 生成核心
│   └── config.py             prompt / 模型配置
├── web/                       React PWA 前端
│   ├── src/
│   │   ├── components/      LoginModal / SearchBar / ...
│   │   ├── api/client.ts     fetch 封装 + token 持久化
│   │   ├── App.tsx           主路由
│   │   └── ...
│   └── public/regions.json    3116 条县区数据
├── data/
│   ├── travelsites.db        SQLite 数据库
│   ├── china_regions_enriched.json  行政区划源数据
│   └── matrix_cache/         JSON 备份（deprecated）
├── .env / .env.example        配置
├── run.py                     入口（uvicorn app.main:app）
└── requirements.txt
```

## 可扩展性设计

| 维度 | 设计 | 未来扩展点 |
|------|------|-----------|
| 节假日 | 国家级 + 扩展字段 (region/demographic) | 地区差异、人群差异 |
| 城市 | SQLite geo_* 三级表 | 加景点库 POI |
| 评分 | 4 维可调权重 | 加节假日、调休、用户偏好 |
| 用户 | role 字段 (user/admin) | 加 editor、vip 等 RBAC |
| Token | UUID 存 DB（可撤销） | 换 JWT、多设备 |

## License

MIT

## 致谢

- Open-Meteo（天气）
- 高德/百度 POI（未来景点数据来源）
- 国务院办公厅（节假日发布）
- 行政区划数据来源 [airyland/china-area-data](https://github.com/airyland/china-area-data)