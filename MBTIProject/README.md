# 测评实验室（MBTIProject）

一个**通用测评平台**：从「猫格测评」起步，但不止于猫格。任何基于问卷的心理/性格测评——MBTI、领导力风格、拖延倾向、狗狗性格——都由同一套「维度 + 题目 + 计分规则」引擎驱动，并支持 **AI 自动生成测评** 与 **测评合理性评价（LLM-as-a-Judge）**。

## 项目结构

```
MBTIProject/
├── backend/                  # FastAPI 后端（Python 3.12+）
│   ├── .venv/                # 虚拟环境
│   ├── requirements.txt
│   ├── .env.example          # LLM 配置示例
│   └── app/
│       ├── main.py           # API 路由
│       ├── schemas.py        # 领域模型（测评/会话/报告）
│       ├── scoring（services/）
│       │   ├── session.py    # 会话流程：开始 → 逐题作答 → 报告
│       │   ├── scoring.py    # 确定性计分引擎 + 报告生成
│       │   ├── generator.py  # ✨ AI 自动生成测评
│       │   └── evaluator.py  # ✨ 测评合理性评价
│       ├── llm.py            # OpenAI 兼容 LLM 封装（ChatGLM/DeepSeek/…）
│       ├── store.py          # JSON 文件存储
│       └── data/             # 内置测评（mbti-lite / cat-personality）
└── frontend/                 # Vite + React + TS 前端
```

## 核心能力

1. **统一测评引擎**：一份测评 = 维度（dimensions）+ 题目（questions）+ 结果模板（types / profile_levels）。支持两种报告类型：
   - `type_matching`：二极轴拼类型码查表（MBTI 的 16 型、猫格的 16 型猫）
   - `dimension_profile`：维度 0-100 分画像（适合领导力等连续特质）
2. **确定性计分**：不依赖 LLM 也能稳定出分出报告；LLM 只负责锦上添花的深度解读。
3. **AI 自动生成测评**：给一个主题（如「职场领导力」），LLM 设计维度、题目、反向计分与结果模板，schema 校验后自动入库，立即可测。
4. **测评合理性评价**：
   - 结构化体检（无 LLM）：题量、维度覆盖、反向题比例（经验区间 10%-50%）、类型码极性可区分性等；
   - LLM 质性评审：内容效度、表述清晰度、计分逻辑三项 0-100 打分 + 问题清单与改进建议。

## 快速开始

### 后端

```bash
cd backend
python3 -m venv .venv
.venv/bin/pip install -r requirements.txt -i https://pypi.tuna.tsinghua.edu.cn/simple
# 可选：配置 LLM（默认走智谱，也支持 DeepSeek/OpenAI 等兼容接口）
cp .env.example .env && $EDITOR .env
.venv/bin/uvicorn app.main:app --reload --port 8000
```

### 前端

```bash
cd frontend
npm install
npm run dev        # http://localhost:5173，已代理 /api → 127.0.0.1:8000
```

## API 一览

| 方法 | 路径 | 说明 |
| --- | --- | --- |
| GET | `/api/assessments` | 测评列表（内置 + AI 生成） |
| GET | `/api/assessments/{id}` | 测评详情 |
| POST | `/api/assessments/{id}/sessions` | 开始一次测评会话 |
| POST | `/api/sessions/{sid}/answers` | 提交一题作答，返回进度或完成态 |
| GET | `/api/sessions/{sid}/report` | 测评报告（`?use_llm=false` 可关闭 AI 解读） |
| POST | `/api/assessments/generate` | ✨ AI 生成测评并入库 |
| POST | `/api/assessments/{id}/evaluation` | ✨ 测评合理性评价 |

交互式文档：http://localhost:8000/docs

## 路线图（承接原 README 的五阶段计划）

- [x] 统一测评引擎与确定性计分（第一阶段基础）
- [x] 逐题作答的多轮问答流程
- [x] LLM 深度报告解读（配置 Key 即启用）
- [x] AI 自动生成测评
- [x] 测评合理性评价（结构化体检 + LLM 评审）
- [ ] 对话式测评：LLM 主导提问，根据历史回答动态决定下一题（原计划第四阶段的自适应提问）
- [ ] 生成测评的「评价 → 自动修订 → 再评价」闭环
- [ ] 数据库存储与用户体系

## 设计说明

- 未配置 `LLM_API_KEY` 时平台完整可用：计分、报告、结构化体检均为确定性逻辑，LLM 相关接口返回明确提示。
- 存储为 JSON 文件（`backend/data/`），原型阶段够用，后续可平滑替换为数据库。
