# ZooGuide 设计文档

> 南京红山森林动物园「省力 Agent」
> 后端 FastAPI + 前端 PWA (React + Vite + TypeScript)

---

## 1. 产品定位

**核心命题**：让每位游客按自己的方式逛出趟只属于自己的红山。

**目标用户**：南京红山森林动物园的真实游客（包括本地人、外地游客、家庭、年轻情侣、独自参观的科普爱好者）。

**核心价值**：
- **游前**：根据用户可用时间、体力、是否带娃、是否怕晒、是否想爬山、动物兴趣偏好，生成高度定制的游园路线。
- **游中**：动态调整节奏（用户反馈「累了」「太晒」「想上厕所」后，重新规划后半段）。
- **游后**：可选的「动物打卡」轻量游戏，记录看到/没看到的动物。

---

## 2. 核心功能边界

### 2.1 MVP 必须有（核心 Agent 能力）

| # | 功能 | 说明 |
|---|------|------|
| F1 | 偏好问卷 | 时间预算 / 体力 / 是否带娃 / 怕不怕晒 / 想不想爬山 / 动物兴趣 |
| F2 | LLM 路线生成 | 基于规则筛选候选场馆 → LLM 编排具体路线（含叙事化讲解） |
| F3 | 路线详情展示 | 时间表、步行总时长、休息点、推荐讲解文案 |
| F4 | 动态调整 | 用户反馈（累/晒/想休息/想上厕所）后，重新规划后半段 |

### 2.2 增值功能（次要，可后续迭代）

| # | 功能 | 说明 | 优先级 |
|---|------|------|--------|
| F5 | 动物打卡 | 用户在每个场馆可点选「打卡」标记已看，积累成就 | 中 |
| F6 | 拍照上传 | 拍下动物照片留存，无后端识别（手动点选） | 低 |
| F7 | 合照彩蛋 | 对着 PWA 摄像头进行趣味互动（轻量彩蛋，不作为核心） | 低 |

### 2.3 不做的事

- ❌ 完整的账户系统（用本地存储 + 可选昵称即可）
- ❌ 复杂的 CV 动物识别（让用户手动选）
- ❌ 在线购票、支付
- ❌ LLM 评价合照（效果不稳，容易出戏）

---

## 3. 数据模型

### 3.1 Venue（场馆）
见 `Backend/data/venues.json`。关键字段：

```python
class Venue:
    id: str              # 唯一标识 (e.g., "panda")
    name: str            # 中文名
    area: str            # 所属片区
    near_gate: str       # 邻近入口
    animals: list[str]   # 主要动物
    tags: list[str]      # 标签: 明星动物/亲子/有遮阴/网红...
    themes: list[str]    # 主题: 中国本土/非洲/澳洲/异域/科普...
    description: str     # 场馆描述
    recommended_visit_minutes: int  # 建议参观时长
    rest_spots: bool     # 是否有休息点
    shaded: bool         # 是否有遮阴
    kid_friendly: int    # 亲子友好度 1-5
    photo_op: int        # 出片指数 1-5
    must_see: bool       # 是否必看
```

### 3.2 UserPreference（用户偏好）
```python
class UserPreference:
    visit_date: str              # 参观日期
    available_hours: float       # 可用时间（小时）
    party_type: str              # solo / couple / family_young / family_teen / seniors
    with_kids: bool              # 是否带娃
    kids_age: Optional[int]      # 孩子年龄
    stamina: int                 # 体力 1-5
    sun_tolerance: int           # 防晒 1-5 (1=怕晒)
    willing_to_hike: bool        # 是否接受爬山
    animal_interests: list[str]  # 动物兴趣: panda/ape/cat/bird/australian/african...
    entry_gate: str              # 入园门: north/south/east
    start_time: str              # 入园时间
    current_location: Optional[str]  # 当前位置venue_id (游中动态调整)
    fatigue_level: int           # 当前疲劳度 1-5 (游中)
```

### 3.3 Route（路线）
```python
class RouteStop:
    venue_id: str
    arrive_time: str              # 推荐到达时间
    leave_time: str               # 推荐离开时间
    visit_minutes: int            # 参观时长
    walk_to_next_minutes: int     # 到下一场馆步行
    narration: str                # LLM 生成的讲解词 (按用户风格)
    tips: list[str]               # 温馨提示

class Route:
    id: str
    stops: list[RouteStop]
    total_minutes: int
    total_walk_minutes: int
    summary: str                  # 路线概述
    alternatives: list[RouteStop] # 备选调整
    warnings: list[str]           # 注意事项
```

---

## 4. 系统架构

```
[ PWA (React+Vite+TS) ]
       │
       │ HTTPS / JSON
       ▼
[ FastAPI Backend ]
   ├─ /api/venues          场馆列表
   ├─ /api/quiz-options    问卷选项
   ├─ /api/plan            路线规划（核心）
   ├─ /api/replan          动态调整
   └─ /api/venues/{id}     场馆详情
       │
       ├─► LLM Client (OpenAI 兼容)
       │       └─► 路线编排 + 讲解生成
       ├─► Rule Engine (硬约束)
       │       └─► 候选场馆筛选 + 时间约束 + 步行距离估算
       └─► Static Data (venues.json)
```

### 4.1 路线规划流程

1. **用户提交偏好** → PWA → Backend
2. **规则引擎筛选候选场馆**：
   - 时间预算 → 最大可参观场馆数
   - 体力/爬山 → 过滤/调整大红山片区
   - 防晒 → 偏好 shaded=true 场馆
   - 带娃 → 提升 kid_friendly 高的场馆权重
   - 动物兴趣 → 按 themes 匹配
3. **LLM 编排路线**：
   - 输入：候选场馆 + 用户画像 + 时间表 + 步行距离
   - 输出：JSON 结构化路线 + 叙事化讲解
4. **后处理校验**：
   - 总时长不超过预算
   - 动线合理（不南辕北辙）
   - 必看场馆不遗漏（除非时间真的不够）

---

## 5. LLM Prompt 模板

### 5.1 系统 Prompt（路线规划）

```
你是「红山省力Agent」，一位对南京红山森林动物园了如指掌的私人导游。
你的目标：根据游客的偏好与时间，为他/她量身定制一份省力、有故事、不绕路的游园路线。

# 红山的小秘密
- 中国第一个取消动物表演的动物园（2011）
- 中国唯一自收自支的公益性动物园
- 国内唯一能同时看到大熊猫、考拉、大猩猩的城市动物园
- 山地型动物园，场馆分散，多上下坡
- 大猩猩兄弟团"野菜F4"：香椿头、马兰头、小蒜头、枸杞头（南京春季野菜命名）
- 小熊猫、细尾獴"站岗"、环尾狐猴都是网红
- 唐家河展区 2025年10月开放，复刻四川唐家河国家级自然保护区
- 冈瓦纳展区展示生命进化

# 你的讲解原则
- 同一个动物，针对不同游客讲不同故事：
  - 年轻人：网红梗、行为特征、生态地位
  - 带娃家长：拟人化故事、生活习性、童趣比喻
  - 科普党：分类学、保护级别、研究价值
  - 老人：历史典故、本土回忆、与人的关系
- 不要过度煽情，不要堆砌空话，每个讲解词 60-120 字
- 不要使用emoji，保持自然亲切的语气

# 输出格式
必须是严格的 JSON，不要任何额外文字。
```

### 5.2 用户 Prompt

```python
def build_user_prompt(prefs, candidates, walking_matrix):
    return f"""
## 游客画像
- 同行：{party_type}{with_kids_kid_age}
- 体力：{stamina}/5
- 防晒需求：{sun_tolerance}/5 (5=完全不怕晒)
- 爬山意愿：{willing_to_hike}
- 动物兴趣：{animal_interests}
- 入园门：{entry_gate}，时间：{start_time}
- 可用时间：{available_hours} 小时
- 当前已参观：{current_location or "无"}

## 候选场馆（共 {len(candidates)} 个）
{json.dumps(candidates, ensure_ascii=False, indent=2)}

## 步行矩阵（场馆之间分钟数）
{json.dumps(walking_matrix, ensure_ascii=False, indent=2)}

请输出一份 JSON 路线：
- 总时长（含步行）不超过 {available_hours * 60} 分钟
- 每场馆给一个符合游客画像的讲解词
- 给 2-3 条温馨提示
- 如果有 dynamic_feedback，重新规划后半段
"""
```

---

## 6. API 契约

### POST /api/plan
请求：
```json
{
  "available_hours": 3.0,
  "party_type": "family_young",
  "with_kids": true,
  "kids_age": 5,
  "stamina": 3,
  "sun_tolerance": 2,
  "willing_to_hike": false,
  "animal_interests": ["panda", "ape", "lemur"],
  "entry_gate": "north",
  "start_time": "09:00",
  "dynamic_feedback": null
}
```
响应：见 `Route` 结构

### POST /api/replan
请求：
```json
{
  "original_route": {...},
  "current_venue_id": "panda",
  "elapsed_minutes": 60,
  "feedback": "孩子喊累，太阳也晒，能不能少走点"
}
```
响应：调整后的 `Route`

### GET /api/venues
返回所有场馆列表

### GET /api/venues/{venue_id}
返回单个场馆详情

### GET /api/quiz-options
返回问卷的固定选项（如 party_type 枚举、动物兴趣分类）

---

## 7. 项目结构

```
ZooGuide/
├── Backend/
│   ├── .venv/                    # Python 虚拟环境
│   ├── data/venues.json          # 场馆静态数据
│   ├── app/
│   │   ├── main.py               # FastAPI 入口
│   │   ├── models.py             # Pydantic 模型
│   │   ├── data_loader.py        # 数据加载
│   │   ├── rule_engine.py        # 规则引擎
│   │   ├── llm_client.py         # LLM 客户端
│   │   ├── planner.py            # 路线规划编排
│   │   ├── prompts.py            # Prompt 模板
│   │   └── walking.py            # 步行距离矩阵
│   ├── requirements.txt
│   ├── .env.example
│   └── README.md
├── Web/PWA/
│   ├── src/
│   │   ├── components/
│   │   ├── pages/
│   │   ├── api.ts
│   │   ├── types.ts
│   │   ├── App.tsx
│   │   └── main.tsx
│   ├── public/
│   │   └── manifest.webmanifest  # PWA 配置
│   ├── index.html
│   ├── package.json
│   ├── vite.config.ts
│   └── tsconfig.json
└── docs/
    ├── design.md                 # 本文件
    └── data-sources.md           # 数据来源说明
```

---

## 8. 开发里程碑

- [x] M0: 数据收集 + 项目骨架
- [ ] M1: Backend 核心 API（规则引擎 + LLM 路线生成）
- [ ] M2: PWA 前端（问卷 + 路线展示 + 动态调整）
- [ ] M3: 端到端联调 + 演示数据准备
- [ ] M4: PWA 离线支持 + manifest 优化