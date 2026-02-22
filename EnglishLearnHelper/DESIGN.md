# 英语学习辅助器 - 设计文档

## 1. 项目概述

### 1.1 项目目标
构建一个基于 AI 的英语学习辅助 Web 应用，帮助用户学习英语词汇、语法、翻译和对话练习。

### 1.2 技术栈
- **前端**: React 19 + TypeScript + Vite
- **后端**: FastAPI (Python)
- **入口**: `run.py`
- **AI 能力**: 集成大语言模型（支持 OpenAI 兼容 API）

---

## 2. 系统架构

```
EnglishLearnHelper/
├── run.py                      # 项目入口
├── backend/
│   ├── app/
│   │   ├── __init__.py
│   │   ├── main.py             # FastAPI 应用
│   │   ├── config.py           # 配置管理
│   │   ├── routers/            # API 路由
│   │   │   ├── __init__.py
│   │   │   ├── vocabulary.py   # 词汇相关 API
│   │   │   ├── translate.py    # 翻译 API
│   │   │   ├── grammar.py      # 语法检查 API
│   │   │   └── chat.py         # AI 对话 API
│   │   ├── services/           # 业务逻辑层
│   │   │   ├── __init__.py
│   │   │   ├── ai_service.py   # AI 服务封装
│   │   │   ├── vocab_service.py
│   │   │   └── user_service.py
│   │   ├── models/             # 数据模型
│   │   │   ├── __init__.py
│   │   │   ├── vocabulary.py
│   │   │   └── user.py
│   │   └── database.py         # 数据库配置
│   └── requirements.txt
├── frontend/
│   └── (现有 React 项目)
└── .env                        # 环境变量
```

---

## 3. 功能模块

### 3.1 词汇学习 (Vocabulary)
| 功能 | 描述 |
|------|------|
| 单词查询 | 查询单词的释义、音标、例句 |
| 单词本 | 用户收藏/管理自己的单词 |
| 每日单词 | 每日推荐学习单词 |
| 记忆复习 | 基于艾宾浩斯遗忘曲线的复习提醒 |

### 3.2 翻译 (Translation)
| 功能 | 描述 |
|------|------|
| 短句翻译 | 中英互译 |
| 段落翻译 | 长文本翻译 |
| 翻译历史 | 保存用户的翻译记录 |

### 3.3 语法检查 (Grammar)
| 功能 | 描述 |
|------|------|
| 句子纠错 | 检查并纠正语法错误 |
| 语法解释 | 解释错误原因和正确用法 |
| 写作建议 | 提供写作改进建议 |

### 3.4 AI 对话 (Chat)
| 功能 | 描述 |
|------|------|
| 情景对话 | 指定场景的英语对话练习 |
| 语音练习 | 口语对话练习（可扩展） |
| 对话评分 | 对用户的回答进行评分和建议 |

---

## 4. API 设计

### 4.1 词汇 API
```
GET  /api/vocabulary/search?word={word}     # 查询单词
POST /api/vocabulary                        # 添加生词
GET  /api/vocabulary                        # 获取单词本列表
DELETE /api/vocabulary/{id}                 # 删除单词
```

### 4.2 翻译 API
```
POST /api/translate
Body: { "text": "Hello", "target_lang": "zh" }
```

### 4.3 语法 API
```
POST /api/grammar/check
Body: { "text": "He go to school yesterday" }
```

### 4.4 对话 API
```
POST /api/chat/message
Body: { "message": "Hello", "scene": "restaurant" }

WebSocket /ws/chat                         # 流式对话
```

---

## 5. 数据模型

### 5.1 Vocabulary (词汇表)
```python
class Vocabulary:
    id: int
    word: str
    phonetic: str          # 音标
    definition: str        # 释义
    example: str            # 例句
    user_id: int
    created_at: datetime
    next_review: datetime  # 下次复习时间
```

### 5.2 User (用户)
```python
class User:
    id: int
    username: str
    email: str
    created_at: datetime
```

---

## 6. AI 服务设计

### 6.1 提示词模板

**单词查询:**
```
你是一个英语词典，请提供以下信息：
单词：{word}
返回格式：JSON
{
  "word": "...",
  "phonetic": "...",
  "definition": "...",
  "example": "..."
}
```

**语法检查:**
```
请检查以下句子的语法错误：
句子：{sentence}
如果有问题，请指出错误并给出正确写法。
```

**情景对话:**
```
你是一个英语口语教练。
场景：{scene}
请用英语与我进行对话练习。
```

### 6.2 API 配置 (.env)
```env
MODEL_KEY=your_api_key
MODEL_URL=https://api.openai.com/v1
MODEL_NAME=gpt-4
```

---

## 7. 前端页面设计

| 页面 | 路由 | 功能 |
|------|------|------|
| 首页 | `/` | 导航入口、功能展示 |
| 查词 | `/vocabulary` | 单词查询界面 |
| 单词本 | `/words` | 我的单词列表 |
| 翻译 | `/translate` | 翻译工具 |
| 语法 | `/grammar` | 语法检查 |
| 对话 | `/chat` | AI 对话练习 |

---

## 8. 后续可扩展功能

- [ ] 用户认证系统
- [ ] 语音识别集成
- [ ] 学习数据分析
- [ ] 单词发音播放
- [ ] 错题本
- [ ] 学习进度统计
- [ ] 社交分享

---

## 9. 开发计划

### Phase 1: 基础框架
- [ ] 后端项目结构搭建
- [ ] FastAPI 基础配置
- [ ] 前端项目初始化
- [ ] 前后端联调

### Phase 2: 核心功能
- [ ] 单词查询功能
- [ ] 翻译功能
- [ ] 语法检查
- [ ] AI 对话

### Phase 3: 用户系统
- [ ] 用户注册/登录
- [ ] 单词本功能
- [ ] 学习记录

---

*文档版本: v1.0*
*创建日期: 2026-02-22*
