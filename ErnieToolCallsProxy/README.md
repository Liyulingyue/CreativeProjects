# Ernie Tool Calls Proxy

这是一个OpenAI格式的LLM代理服务，用于将不支持原生Tool Calls功能的模型（如部分开源或定制LLM）包装为支持Tool Calls的接口。通过在代理过程中修改模型输入（注入工具调用提示）和输出（解析文本中的工具调用），实现与OpenAI API兼容的工具调用体验。

例如，百度的文心一言 4.5版本（Ernie Bot 4.5）目前不支持原生的Tool Calls功能，但通过本代理，可以实现类似的功能。

## 项目概述

当前目录包含以下主要文件：
- `OpenAIProxy.py`：基本的 OpenAI 代理服务器。
- `OpenAIProxyToolCalls.py`：扩展的工具调用包装器服务器，支持解析和返回工具调用。
- `test_Proxy.py` 和 `test_ProxyToolCalls.py`：测试文件。
- `requirements.txt`：项目依赖。
- `miniCoder.py`：迷你Claude 的代理实现。

如果你希望代理大模型服务，你可以参考 `OpenAIProxy.py` 文件，并使用 `test_Proxy.py` 进行测试。
如果你希望扩展大模型服务，例如提供 OpenAI 风格的调用接口，你可以参考 `OpenAIProxyToolCalls.py` 文件了解如何在代理中增加额外功能，并使用 `test_ProxyToolCalls.py` 进行测试。

## 快速使用文心一言4.5和Tool Calls 实现工具调用
1. 安装依赖
    ```bash
    pip install -r requirements.txt
    ```
2. 在 .env 文件或环境变量中设置文心一言 API Key (如果不方便配置 .env 文件，可以直接修改代码中的环境变量读取部分)
    ```text
    MODEL_URL=http://localhost:8000/v1
    MODEL_KEY=*****************************
    MODEL_NAME=ernie-4.5-vl-28b-a3b
    ```
3. 运行代理服务器
    ```bash
    python OpenAIProxyToolCalls.py
    ```
4. 运行 MiniCoder
    ```bash
    python miniCoder.py
    ```

调用示例如下：
```text 
$ python ./miniCoder.py
>> hi
hi! What can I assist you with today via bash commands?
>> show all folder
$ ls -d */
__pycache__/

I executed the command `ls -d */` to list all directories. Here's the output:

__pycache__/

Let me know if you'd like to perform any other file-related operations.
```

可以看到，MiniCoder 成功调用了文心一言4.5模型，Ernie 4.5 模型通过代理实现了 Tool Calls 功能实现了一个微缩版的工具调用。

## 如何代理 OpenAI

### 1. 环境依赖
- Python 版本 >= 3.8
- 依赖库：`fastapi`、`uvicorn`、`openai`

### 2. 转发机制

#### 2.1 解析请求
- 接收客户端的OpenAI风格请求，提取关键参数如 `messages`、`tools`、`model` 等。
- 如果需要工具调用，修改提示以注入工具调用指令。
- **核心元素获取方式**：
  - **请求体解析**：使用 `await req.body()` 获取原始字节流，然后 `json.loads(raw_bytes)` 解析为JSON对象。
  - **messages**：从JSON中提取 `body_json.get("messages", [])`，这是一个消息列表，包含用户输入和对话历史。
  - **model**：提取 `body_json.get("model", "")`，指定使用的模型名称，如 "ernie-bot-4.5"。
  - **tools**：提取 `body_json.get("tools", [])`，这是一个工具定义列表，每个工具包含 `type` 和 `function` 字段。
  - **max_tokens**：提取 `body_json.get("max_tokens", 0)`，限制生成的最大token数。
  - **API密钥**：从请求头中提取 `headers.get("authorization", "").replace("Bearer ", "")`，用于后端认证。
  
#### 2.2 二次转发
- 使用OpenAI SDK或直接HTTP请求将修改后的请求发送到后端LLM。
- **示例转发请求代码**（伪代码）：
  ```python
    # proxy to https://aistudio.baidu.com/llm/lmapi/v3
    client = OpenAI(api_key=api_key, base_url=PROXYED_URL)
    response = client.chat.completions.create(
        model=model,
        messages=messages,
        tools=tools,
        max_tokens=max_tokens
    )
  ```

#### 2.3 封装回复
- 接收后端响应，解析并转换为标准OpenAI格式的JSON响应。
    ```python
        serializable = response.to_dict()
        return JSONResponse(content=serializable)
    ```


### 完整代理服务器样例
```python
    import os
    import json
    from datetime import datetime
    from openai import OpenAI
    from fastapi import FastAPI, Request
    from fastapi.responses import JSONResponse
    app = FastAPI(title="OpenAI Proxy Debugger")

    PROXYED_URL = os.getenv("PROXYED_URL", "https://aistudio.baidu.com/llm/lmapi/v3")


    @app.post("/v1/chat/completions")
    async def proxy_chat_completions(req: Request):
        ts = datetime.utcnow().isoformat() + "Z"

        # headers
        headers = dict(req.headers)
        api_key = headers.get("authorization", "").replace("Bearer ", "")

        # raw_body as JSON
        raw_bytes = await req.body()
        try:
            body_json = json.loads(raw_bytes)
        except Exception:
            body_json = None
        messages = body_json.get("messages", []) if body_json else []
        model = body_json.get("model", "") if body_json else ""
        tools = body_json.get("tools", []) if body_json else []
        max_tokens = body_json.get("max_tokens", 0) if body_json else 0

        # proxy to https://aistudio.baidu.com/llm/lmapi/v3
        client = OpenAI(api_key=api_key, base_url=PROXYED_URL)
        response = client.chat.completions.create(
            model=model,
            messages=messages,
            tools=tools,
            max_tokens=max_tokens
        )

        # Serialize the response using to_dict() as confirmed working
        serializable = response.to_dict()

        return JSONResponse(content=serializable)

    @app.get("/health")
    async def health():
        return {"ok": True}


    if __name__ == "__main__":
        import uvicorn
        uvicorn.run("OpenAIProxy:app", host="0.0.0.0", port=9000, reload=True)
```

### 调用代理服务器
下面是一个使用代理服务器并调用文心一言4.5模型的示例代码：
```python
from openai import OpenAI

api_key = "*******************************"
base_url = "http://localhost:9000/v1"
model_name = "ernie-4.5-21b-a3b"

client = OpenAI(api_key=api_key, base_url=base_url)
TOOL = [{
    "type": "function",
    "function": {
        "name": "bash", 
        "description": """Execute shell command.""",
        "parameters": {
            "type": "object", 
            "properties": {
                "command": {"type": "string"}
            }, 
            "required": ["command"]
        }
    }
}]
SYSTEM = f"""You are an AI assistant that helps the user run bash commands on their computer."""

history = []
prompt = "hi"
history.append({"role": "user", "content": prompt})
r = client.chat.completions.create(model=model_name, messages=history, tools=TOOL, max_tokens=8000)
message = r.choices[0].message
```

## 在代理基础上扩展 Tools Call

### 2.1 解析请求(不变)
- 接收客户端的OpenAI风格请求，提取关键参数如 `messages`、`tools`、`model` 等。
- 如果需要工具调用，修改提示以注入工具调用指令。
- **核心元素获取方式**：
  - **请求体解析**：使用 `await req.body()` 获取原始字节流，然后 `json.loads(raw_bytes)` 解析为JSON对象。
  - **messages**：从JSON中提取 `body_json.get("messages", [])`，这是一个消息列表，包含用户输入和对话历史。
  - **model**：提取 `body_json.get("model", "")`，指定使用的模型名称，如 "ernie-bot-4.5"。
  - **tools**：提取 `body_json.get("tools", [])`，这是一个工具定义列表，每个工具包含 `type` 和 `function` 字段。
  - **max_tokens**：提取 `body_json.get("max_tokens", 0)`，限制生成的最大token数。
  - **API密钥**：从请求头中提取 `headers.get("authorization", "").replace("Bearer ", "")`，用于后端认证。
  
### 2.2 二次转发(扩展)
- 在原有messages输入的基础上，对assistant和toolcall信息做了合并、对提示词做了修改。
- **消息合并逻辑**：将assistant消息中的tool_calls和后续的tool消息合并到assistant的content中，便于后端模型理解上下文。
- **提示词修改**：在system消息中注入工具调用格式化指令，要求模型在需要调用工具时使用特定标记输出JSON。
- **具体代码**：
  ```python
  # 处理 tool_calls 和 tool 消息：合并 assistant 和 tool 消息
  processed_messages = []
  i = 0
  while i < len(messages):
      msg = messages[i]
      if msg.get("role") == "assistant" and "tool_calls" in msg:
          # 合并 tool_calls 到 content，并检查下一个是否是 tool 消息
          tool_calls_str = json.dumps(msg["tool_calls"])
          content = msg.get("content", "") + f"\n[Tool Calls: {tool_calls_str}]"
          # 检查下一个消息
          if i + 1 < len(messages) and messages[i + 1].get("role") == "tool":
              tool_msg = messages[i + 1]
              content += f"\n[Tool Result: {tool_msg.get('content', '')}]"
              i += 1  # 跳过 tool 消息
          processed_messages.append({"role": "assistant", "content": content})
      elif msg.get("role") == "tool":
          # 如果是孤立的 tool 消息，跳过
          pass
      else:
          processed_messages.append({"role": msg.get("role"), "content": msg.get("content", "")})
      i += 1

  messages = processed_messages

  # 注入格式化输出提示
  fmt_instr = (
      "When you need to call tools, append at the end of your text a JSON array of tool calls between the markers:\n"
      f"{TOOL_MARKER_START}\n"
      "[{\"id\":\"call_1\", \"name\":\"tool_name\", \"arguments\": {\"param\": \"value\"}}]\n"
      f"{TOOL_MARKER_END}\n"
      "The content before the markers is the user-facing text. Do not include any extraneous commentary inside the markers."
      "The tools you can call are: \n"
      f"{tools}.\n"
      "You must think carefully about the aim of user."
      "If you need external information or actions, use the tools provided."
      "otherwise, just respond directly without the above markers."
  )
  # 如果有 system 消息，则将提示添加到 system content，否则插入 system 消息
  system_msgs = [m for m in messages if m.get("role") == "system"]
  if system_msgs:
      system_msgs[0]["content"] = f"{fmt_instr}\n\n{system_msgs[0]['content']}"
  else:
      messages.insert(0, {"role": "system", "content": fmt_instr})
  ```

### 2.3 封装回复(扩展)
- 对输出的纯文本内容做了拆分，并重新组织了toolcalls格式输出。
- **如何解析**：使用解析函数从模型输出的纯文本中提取工具调用信息，分离出用户可见的内容和工具调用JSON。支持标记块解析、尾部JSON解析等策略。
- **如何组织**：将解析出的工具调用转换为OpenAI标准的tool_calls数组格式，包括id、type、function字段，并构建完整的响应结构。
- **具体代码**：
  ```python
    # 步骤1: 解析内嵌的工具调用
    # 从模型返回的纯文本content中解析出工具调用信息
    def parse_embedded_tool_calls(text: str) -> Tuple[str, list, Optional[str]]:
        """解析模型文本中内嵌的 tool_calls 块。
        返回 (clean_content, tool_calls_list, parse_error)

        支持的格式（按优先级）：
        1) 明确的标记块：--TOOL_CALLS_START-- ...JSON... --TOOL_CALLS_END--
        2) 文本末尾直接是一个 JSON 对象或数组（尝试解析最后一个 JSON 对象/数组）
        3) 解析失败则返回原始文本与错误信息

        解析的JSON格式假设为：[{"id": "call_id", "name": "func_name", "arguments": {...}}]
        将其转换为OpenAI标准的tool_calls格式。
        """
        if not text:
            return text, [], None

        def normalize_tool_calls(parsed):
            """将自定义格式转换为OpenAI标准格式"""
            if not isinstance(parsed, list):
                parsed = [parsed]
            normalized = []
            for item in parsed:
                if isinstance(item, dict) and "name" in item and "arguments" in item:
                    normalized.append({
                        "id": item.get("id", f"call_{len(normalized)}"),
                        "type": "function",
                        "function": {
                            "name": item["name"],
                            "arguments": json.dumps(item["arguments"]) if isinstance(item["arguments"], dict) else item["arguments"]
                        }
                    })
            return normalized

        # 1) 标记块
        start_idx = text.find(TOOL_MARKER_START)
        end_idx = text.find(TOOL_MARKER_END)
        if start_idx != -1 and end_idx != -1 and end_idx > start_idx:
            inner = text[start_idx + len(TOOL_MARKER_START):end_idx].strip()
            try:
                parsed = json.loads(inner)
                clean = (text[:start_idx].strip())
                tool_calls = normalize_tool_calls(parsed)
                return clean, tool_calls, None
            except Exception as e:
                return text, [], f"marker json parse error: {e}"
        else:
            return text, [], None

    parsed_content, tool_calls, parse_error = parse_embedded_tool_calls(content)
    
    # 更新content为解析后的干净内容（移除工具调用标记）
    content = parsed_content
    
    # 步骤2: 组织响应结构
    # 构建OpenAI标准的chat completion响应格式
    response = {
        "id": r_id or None,  # 响应ID
        "object": "chat.completion",  # 对象类型
        "created": int(time.time()),  # 创建时间戳
        "model": r_model,  # 使用的模型
        "choices": [
            {
                "index": 0,  # 选择索引
                "message": {
                    "role": "assistant",  # 消息角色
                    "content": content,  # 解析后的用户可见内容
                    "tool_calls": tool_calls,  # 标准化的工具调用列表
                    "raw_model_text": raw_model_text,  # 原始模型输出（调试用）
                    "parse_error": parse_error,  # 解析错误信息（调试用）
                },
                "finish_reason": finish_reason,  # 完成原因
            }
        ],
    }
  ```

## 总结
通过本项目，可以将不支持原生Tool Calls功能的LLM模型包装为支持OpenAI风格工具调用的接口。关键在于在请求阶段注入工具调用提示，并在响应阶段解析模型输出中的工具调用信息。这样，用户可以使用熟悉的OpenAI API格式与各种LLM模型进行交互，实现更丰富的功能。

经测试，当前项目中的代理组织方式只能用于简单的工具调用场景，暂无法实现复杂和稳定的工具调用流程。

## 参考
- [相关文章](https://mp.weixin.qq.com/s/WPkCONFnBc84Q3V5Qjynrg)：MiniCoder的实现。