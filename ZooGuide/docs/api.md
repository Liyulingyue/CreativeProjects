# ZooGuide API 契约

> Base URL：`http://localhost:8000/api`
> 所有响应均为 JSON。错误格式：`{"error": "...", "detail": "..."}`

---

## 1. 元信息

### GET /meta
动物园基础信息（开园时间、门票、片区分布等）。

**响应**：
```json
{
  "name": "南京红山森林动物园",
  "open_time": "08:30",
  "close_time": "16:30",
  "highlights": ["...", "..."],
  "gates": ["北门", "南门", "东门"]
}
```

---

## 2. 场馆

### GET /venues
返回所有场馆列表（精简字段）。

**响应**：
```json
{
  "venues": [
    {
      "id": "panda",
      "name": "大熊猫馆",
      "area": "大红山片区",
      "animals": ["大熊猫"],
      "tags": ["国宝", "明星动物"],
      "must_see": true
    }
  ]
}
```

### GET /venues/{venue_id}
返回场馆详情。

---

## 3. 问卷选项

### GET /quiz-options
返回问卷的固定选项（前端懒加载一次即可）。

**响应**：
```json
{
  "party_types": [
    {"value": "solo", "label": "一个人", "icon": "🧍", "desc": "专注观察动物，按自己节奏走"},
    {"value": "couple", "label": "情侣/朋友", "icon": "👥", "desc": "兼顾出片与轻松"},
    {"value": "family_young", "label": "带学龄前娃", "icon": "👨‍👩‍👧", "desc": "节奏要慢，多亲子互动"},
    {"value": "family_teen", "label": "带青少年", "icon": "🧑‍🎓", "desc": "可步行更多，可加科普"},
    {"value": "seniors", "label": "带老人", "icon": "👵", "desc": "少爬坡，多座椅与厕所"}
  ],
  "interests": [
    {"value": "panda", "label": "国宝大熊猫"},
    {"value": "ape", "label": "灵长类"},
    {"value": "cat", "label": "猫科动物"},
    {"value": "bird", "label": "鸟类"},
    {"value": "australian", "label": "澳洲动物"},
    {"value": "african", "label": "非洲动物"},
    {"value": "local", "label": "中国本土物种"},
    {"value": "exotic", "label": "异域奇观"},
    {"value": "kids_favorite", "label": "孩子最爱"}
  ],
  "gates": [
    {"value": "north", "label": "北门", "desc": "地铁1号线，最近大熊猫馆"},
    {"value": "south", "label": "南门", "desc": "2025新馆区，非洲/唐家河/大猩猩"},
    {"value": "east", "label": "东门", "desc": "高黎贡、冈瓦纳"}
  ],
  "stamina_descriptions": {
    "1": "体力一般，不想走太多",
    "2": "偏休闲，可走 1-2 公里",
    "3": "一般，可走 3-4 公里",
    "4": "较好，可走 5-6 公里",
    "5": "精力充沛，可暴走全园"
  },
  "sun_descriptions": {
    "1": "非常怕晒，必须阴凉/室内",
    "2": "怕晒，倾向遮阴路线",
    "3": "无所谓",
    "4": "能晒",
    "5": "喜欢阳光户外"
  }
}
```

---

## 4. 路线规划（核心）

### POST /plan
根据用户偏好生成一条定制路线。

**请求**：
```json
{
  "available_hours": 3.0,
  "party_type": "family_young",
  "with_kids": true,
  "kids_age": 5,
  "stamina": 3,
  "sun_tolerance": 2,
  "willing_to_hike": false,
  "animal_interests": ["panda", "ape", "kids_favorite"],
  "entry_gate": "north",
  "start_time": "09:00"
}
```

**响应**：
```json
{
  "id": "r_xxx",
  "summary": "今天适合轻松逛北门到大红山的核心明星路线...",
  "total_minutes": 175,
  "total_walk_minutes": 35,
  "stops": [
    {
      "venue_id": "panda",
      "venue_name": "大熊猫馆",
      "arrive_time": "09:05",
      "leave_time": "09:35",
      "visit_minutes": 30,
      "walk_to_next_minutes": 3,
      "narration": "咱们第一站就去看'国宝'...",
      "tips": ["馆内有空调，可先在这里凉快一下"],
      "rest_here": false
    },
    {
      "venue_id": "meerkat",
      "venue_name": "细尾獴馆",
      "arrive_time": "09:38",
      "leave_time": "09:53",
      "visit_minutes": 15,
      "walk_to_next_minutes": 4,
      "narration": "小朋友最爱的'站岗小哨兵'...",
      "tips": ["可以让孩子蹲下来和它们对视"],
      "rest_here": false
    }
  ],
  "warnings": ["园内禁止投喂动物", "请勿使用闪光灯"],
  "tips": ["带娃节奏建议每 1.5 小时休息一次"],
  "fallback": false
}
```

**字段含义**：
- `summary`：路线的整体导览文字（叙事化）
- `total_minutes`：包含参观+步行的总时长
- `total_walk_minutes`：纯步行时长
- `stops[].narration`：针对该用户的个性化讲解
- `stops[].tips`：在该场馆的温馨提示
- `stops[].rest_here`：是否建议在此处坐下来歇脚
- `fallback`：true 表示 LLM 未调用，使用规则引擎回退
- `warnings`：通用注意事项
- `tips`：针对该用户画像的导览建议

**错误码**：
- `400`：参数非法
- `500`：LLM/内部错误（此时仍会返回带 `fallback: true` 的响应）

---

### POST /replan
游中动态调整。给定当前已参观进度和反馈，重新生成后半段路线。

**请求**：
```json
{
  "original_route": { /* 上一次 plan 响应 */ },
  "current_venue_id": "panda",
  "elapsed_minutes": 60,
  "feedback": "孩子喊累，太阳也晒了，能不能少走点？"
}
```

**响应**：与 `/plan` 同结构。但只保留当前场馆之后的内容（前半段已固定在历史中）。

---

## 5. 动物打卡

### POST /checkin
记录一次动物打卡（不上传图片）。

**请求**：
```json
{
  "venue_id": "panda",
  "session_id": "uuid-v4"
}
```

**响应**：
```json
{
  "ok": true,
  "total_checkins": 3,
  "venue_name": "大熊猫馆"
}
```

### GET /checkin/{session_id}
获取当前 session 的打卡记录。

**响应**：
```json
{
  "session_id": "uuid",
  "checkins": [
    {"venue_id": "panda", "venue_name": "大熊猫馆", "ts": "2026-07-10T14:23:00"}
  ],
  "completion_rate": 0.13
}
```

---

## 6. 健康检查

### GET /health
```json
{
  "status": "ok",
  "use_llm": true,
  "model": "gpt-4o-mini",
  "venue_count": 22
}
```

---

## 7. 版本

`v1`（MVP）。后续版本通过 `/api/v2/...` 演进。