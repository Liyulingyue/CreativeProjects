# MiniCoder — 极简 LLM 代码助理

轻量化的 MiniCoder：模块化 Agent + LLM 客户端，提供四条命令行功能 — `generate`, `explain`, `fix`, `optimize`。

## 安装

```bash
pip install -r requirements.txt
cp .env.example .env    # 编辑并添加 MODEL_KEY
```

## 快速使用（CLI）

- 生成代码:
  ```bash
  python mini_coder.py generate -p "实现快速排序" -l python
  ```

- 解释代码:
  ```bash
  python mini_coder.py explain -c "def foo(): ..."
  ```

- 修复 bug:
  ```bash
  python mini_coder.py fix --error "IndexError" --context "arr=[1]; print(arr[2])"
  ```

- 优化代码:
  ```bash
  python mini_coder.py optimize -c "def f(): pass"
  ```

> ⚠️ 需要在环境变量中设置 `MODEL_KEY`（或写入 `.env`）。

## 文件说明

- `agent.py` — Agent 风格实现（业务逻辑）
- `llm_client.py` — LLM wrapper（集中化 API 调用）
- `mini_coder.py` — 极简 CLI（入口）
- `tools.py` — 辅助工具函数

---
