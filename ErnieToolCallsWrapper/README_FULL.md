# Ernie Tool Calls Wrapper

这是一个小型的代理服务，用来把后端支持 tool_calls 的 LLM 的返回转换成通用的 `content` + `tool_calls` 结构，方便不直接支持 tool_calls 的客户端使用。

## 快速开始

1. 设置环境变量（示例）：

```bash
export MODEL_KEY="sk-..."          # 后端 LLM 的 API key
export MODEL_URL="https://api.openai.com/v1"  # 或者你的兼容后端
export MODEL_NAME="gpt-4"
# 注意：本 wrapper 不会在服务器端自动执行工具调用，仅解析并返回 tool_calls
```

2. 安装依赖并运行（推荐在虚拟环境中）：

```bash
pip install fastapi uvicorn openai
python ToolcallsWrapperServer.py
# 或者使用 uvicorn 直接运行
uvicorn ToolcallsWrapperServer:app --reload --port 8000
```

3. 调用示例（简单）：

```bash
curl -sS -X POST "http://localhost:8000/v1/chat/completions" \
  -H "Content-Type: application/json" \
  -d '{"model":"gpt-4","messages":[{"role":"user","content":"show the folder"}], "tools": [{"type":"function","function": {"name":"bash","description":"run shell","parameters":{"type":"object","properties":{"command":{"type":"string"}},"required":["command"]}}}] }'
```

你也可以在请求体中传入 `api_key` 和 `base_url` 来覆盖环境变量：

```bash
curl -sS -X POST "http://localhost:8000/v1/chat/completions" \
  -H "Content-Type: application/json" \
  -d '{"model":"gpt-4","api_key":"sk-...","base_url":"https://api.openai.com/v1","messages":[{"role":"user","content":"show the folder"}], "tools": [...] }'
```

返回示例（关键字段）：

```json
{
  "choices": [
    {
      "message": {
        "content": "...",
        "tool_calls": [
          {"id": "call_xxx", "name": "bash", "arguments": {"command": "ls -la"}, "raw": {...}}
        ],
        "content_with_tool_calls": "...包含序列化的 tool_calls ..."
      }
    }
  ]
}
```

## 注意事项与安全

- 本 wrapper **不会** 在服务器端执行收到的工具调用；它仅解析并以统一的 `tool_calls` 结构返回。若需要执行工具，请在受信任和隔离的环境中自行实现且严格限制允许的命令。

- 本项目为开发/实验用途，不建议直接在生产环境运行.


## 强制格式化输出（FORCE_TOOL_FORMAT）

代理支持在转发前注入一个提示（默认启用），要求模型在需要调用工具时在输出末尾用固定标记包含 JSON 格式的工具调用块。该行为由环境变量 `FORCE_TOOL_FORMAT` 控制（默认 `true`），标记为：

```
--TOOL_CALLS_START--
...JSON array/object...
--TOOL_CALLS_END--
```

代理会优先使用模型原生返回的 `tool_calls`（若后端支持），否则会回退到从文本中解析上述标记或末尾 JSON 的解析器。
