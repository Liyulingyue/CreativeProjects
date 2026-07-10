# ZooGuide 编码规范与约束

> 本文档是开发的"硬约束"。所有 PR、commit 必须遵循。

---

## 1. 项目骨架约束

```
ZooGuide/
├── Backend/                FastAPI 后端（Python 3.12, .venv 隔离）
├── Web/PWA/                React + Vite + TypeScript 前端
├── docs/                   设计/数据/API 文档
└── README.md               总入口
```

### 1.1 Backend 约束

- **必须使用 `.venv`**：所有依赖装在 `Backend/.venv/`
- **入口**：`Backend/run.py` 或 `uvicorn app.main:app`
- **目录划分**：
  - `app/`：FastAPI 路由、main、config
  - `src/`：核心库（planner, rule_engine, llm_client, prompts, data_loader）
  - `data/`：静态 JSON 数据
- **LLM 调用统一通过 `OpenAIJsonWrapper`**，禁止裸调 OpenAI client
- **Pydantic v2** 做所有外部输入校验

### 1.2 Frontend 约束

- **React 18+ + Vite + TypeScript**
- **PWA manifest** 必备（`public/manifest.webmanifest`）
- **状态管理**：优先 useState/Context，必要时 Zustand，不引入 Redux
- **UI 库**：可选 `tailwindcss`，禁止引入 antd / mui 等重量级库
- **图标**：纯 SVG 或 lucide-react，不引入 iconfont 字体
- **路由**：可选 react-router-dom，简单多步流程可不引入（用 state 切页）

---

## 2. LLM 调用约束

### 2.1 必须遵循的模式

```python
# ✅ 正确：通过 OpenAIJsonWrapper
from openai import OpenAI
from openaijsonwrapper import OpenAIJsonWrapper

client = OpenAI(api_key=API_KEY, base_url=BASE_URL)
wrapper = OpenAIJsonWrapper(
    client,
    model=MODEL_NAME,
    target_structure=...,
    background=...,
    requirements=[...],
)
result = wrapper.chat(messages=messages)
# result = {"error": ..., "data": ..., "reasoning": ..., "raw_content": ...}

# ❌ 错误：裸调 client
response = client.chat.completions.create(...)
json.loads(response.choices[0].message.content)
```

### 2.2 Prompt 模板必须分离

所有 prompt 模板（`background` / `requirements` / `target_structure`）必须放在 `src/prompts.py` 中，**禁止硬编码在业务逻辑里**。

### 2.3 必须有的 LLM 兜底

`USE_LLM=false` 时，`/api/plan` 必须返回基于规则的"基础版路线"，保证 demo 永远可跑。LLM 出错时降级到规则版本。

---

## 3. 数据约束

### 3.1 来源

- 场馆主数据：`Backend/data/venues.json`（手工建模，已完成）
- 来源标注：`docs/data-sources.md`
- 静态、不依赖数据库（避免 SQLite schema 演进）

### 3.2 字段稳定

每个 venue 必有以下字段，禁止随意增减（避免破坏 LLM prompt）：
```
id, name, area, near_gate, open_time, close_time,
animals, tags, themes, description,
recommended_visit_minutes, rest_spots, shaded,
kid_friendly, photo_op, must_see
```

---

## 4. API 契约约束

### 4.1 路由前缀

所有路由必须挂在 `/api/` 前缀下。

### 4.2 错误响应格式

```json
{"error": "human-readable message", "detail": "..."}
```

### 4.3 CORS

默认允许 `localhost:5173`（Vite dev server）。生产由 Nginx 同源反向代理，无需 CORS。

---

## 5. 前端约束

### 5.1 不依赖任何外部地图 SDK

MVP 不集成高德/百度地图。路线展示用列表 + 时间线即可，避免引入额外 token 与 key。

### 5.2 本地存储

- `localStorage` 用于：用户偏好记忆、上次入园门、暗色模式、动物打卡记录
- **禁止**：把 token 存 localStorage（PWA 没有后端用户系统，不涉及）

### 5.3 PWA 必备项

- `manifest.webmanifest`（name, short_name, icons[192, 512], theme_color, display=standalone）
- `service-worker.js`：缓存壳（HTML/CSS/JS），缓存策略 stale-while-revalidate
- iOS 适配：`<meta name="apple-mobile-web-app-capable">`

---

## 6. Commit 约束

- **每完成一个独立功能节点就 commit**
- commit message 格式：`<scope>: <简明描述>`
  - `backend: add planner core with LLM rule fallback`
  - `web: add PWA questionnaire flow`
  - `docs: add API contract spec`
- **不 commit**：
  - `.venv/`
  - `node_modules/`
  - `dist/`
  - `.env`（保留 `.env.example`）
  - 大型构建产物

---

## 7. 验收标准（明早 8:00）

### 必须可演示
1. ✅ 启动 `Backend`：8000 端口可访问 `/docs`
2. ✅ 启动 `Web/PWA`：5173 端口可访问，manifest 注册成功
3. ✅ 完整流程：问卷 → 路线规划 → 动态调整 → 动物打卡

### 可演示加分项
- LLM 关闭时也能跑通（规则引擎回退）
- 路线详情页有可点开的讲解词
- PWA 可"添加到主屏幕"

---

## 8. 范围之外（不做的）

- ❌ 真实 CV 识别动物（不引入 YOLO/CV 模型）
- ❌ 实时天气对接（MVP 不接 Open-Meteo，让 LLM 给提示）
- ❌ 完整用户系统（localStorage 足够）
- ❌ 支付/购票
- ❌ 多动物园切换（聚焦红山）
- ❌ 国际化（仅中文）