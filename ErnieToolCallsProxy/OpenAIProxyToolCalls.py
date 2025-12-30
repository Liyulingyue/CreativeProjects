#!/usr/bin/env python3
"""Tool Calls Wrapper Server

- 接受 OpenAI 风格的 /v1/chat/completions 请求
- 将请求转发到后端 LLM（通过 OpenAI SDK，可配置 BASE URL）
- 解析返回的 message.content 与 message.tool_calls（如果存在）
- 返回统一的 JSON：在 message 中同时包含原始 content 和抽取的 tool_calls
- 可选：在服务器端执行 tool_calls（受环境变量开关控制）

安全提示：工具执行功能会运行外部命令，仅在受信任环境下启用。"""

import os
import time
import json
from typing import Optional, Any

from fastapi import FastAPI, Request, HTTPException
from fastapi.responses import JSONResponse
from pydantic import BaseModel

try:
    from openai import OpenAI
except Exception as e:
    OpenAI = None  # 稍后会在运行时检查

# 格式化输出的标记（注：解析时会查找这组标记）
TOOL_MARKER_START = "--TOOL_CALLS_START--"
TOOL_MARKER_END = "--TOOL_CALLS_END--"  # 不执行工具，仅解析并返回 tool_calls
PROXYED_URL = os.getenv("PROXYED_URL", "https://aistudio.baidu.com/llm/lmapi/v3")

app = FastAPI(title="Tool Calls Wrapper")

class ChatCompletionRequest(BaseModel):
    model: Optional[str] = "user_model"
    messages: list
    max_tokens: Optional[int] = 1000
    temperature: Optional[float] = 0.0
    api_key: Optional[str] = None
    base_url: Optional[str] = None
    # allow arbitrary extra fields
    class Config:
        extra = "allow"

# 解析器：如果后端模型不原生返回 tool_calls，我们会尝试从 text content 中解析内嵌的工具调用
import re
from typing import Tuple

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

    # 2) 尝试从文本尾部提取最后一个 JSON array/object
    # 找到最后一个 '[' 或 '{' 并尝试解析到末尾
    for opener, closer in (("[", "]"), ("{", "}")):
        idx = text.rfind(opener)
        if idx != -1:
            cand = text[idx:].strip()
            try:
                parsed = json.loads(cand)
                clean = text[:idx].strip()
                tool_calls = normalize_tool_calls(parsed)
                return clean, tool_calls, None
            except Exception:
                # 继续尝试下一个
                pass

    # 3) 回退：尝试通过正则匹配最后一段类似 JSON 的文本（宽容）
    m = re.search(r"(\[[\s\S]*\]|\{[\s\S]*\})\s*$", text)
    if m:
        cand = m.group(1)
        try:
            parsed = json.loads(cand)
            clean = text[:m.start(1)].strip()
            tool_calls = normalize_tool_calls(parsed)
            return clean, tool_calls, None
        except Exception as e:
            return text, [], f"trailing json parse error: {e}"

    # 无解析结果
    return text, [], None

@app.post("/v1/chat/completions")
async def chat_completions(req: Request):
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
    max_tokens = body_json.get("max_tokens", 32000) if body_json else 32000
    base_url = body_json.get("base_url") or body_json.get("baseUrl") or body_json.get("url") if body_json else None
    temperature = body_json.get("temperature", 0.7) if body_json else 0.7

    # 检查messages是否最后一条是用户信息，否则增加一条空的用户信息
    if not messages or messages[-1].get("role") != "user":
        messages.append({"role": "user", "content": ""})

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

    # 动态创建客户端
    client = OpenAI(api_key=api_key, base_url=PROXYED_URL)

    # 注入格式化输出提示（系统消息或用户消息，视现有消息而定）
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

    print(f"DEBUG: Final messages sent to backend:\n{json.dumps(messages, indent=2)}")

    # 直接采用OpenAI SDK进行调用
    r = client.chat.completions.create(
        model=model,
        messages=messages,
        max_tokens=max_tokens,
        temperature=temperature,
    )
    
    print(r.choices[0].message)

    # 解析返回（兼容 SDK 对象与 HTTP 转发的 dict）
    try:
        # 取第一个 choice（支持 dict 或 SDK 对象）
        if isinstance(r, dict):
            choices_list = r.get("choices", [])
            choice = choices_list[0] if choices_list else {}
            r_id = r.get("id")
            r_model = r.get("model") or model
        else:
            choices_list = getattr(r, "choices", [])
            choice = choices_list[0] if choices_list else None
            r_id = getattr(r, "id", None)
            r_model = getattr(r, "model", None) or model

        # 提取 message/content
        if isinstance(choice, dict):
            message = choice.get("message")
            content = (message.get("content") if isinstance(message, dict) else None) or choice.get("text") or ""
        else:
            message = getattr(choice, "message", None)
            content = (getattr(message, "content", None) if message else None) or getattr(choice, "text", None) or ""

        raw_model_text = content

        # 直接解析内嵌的 tool_calls
        parsed_content, tool_calls, parse_error = parse_embedded_tool_calls(content)
        # print(f"DEBUG: Original content: {content}")
        # print(f"DEBUG: Parsed content: {parsed_content}")
        # print(f"DEBUG: Tool calls: {tool_calls}")
        # print(f"DEBUG: Parse error: {parse_error}")
        content = parsed_content  # 总是更新为解析后的内容（成功时移除标记，失败时保持原始）

        response = {
            "id": r_id or None,
            "object": "chat.completion",
            "created": int(time.time()),
            "model": r_model,
            "choices": [
                {
                    "index": 0,
                    "message": {
                        "role": "assistant",
                        "content": content,
                        "tool_calls": tool_calls,
                        "raw_model_text": raw_model_text,  # 非必要字段
                        "parse_error": parse_error,  # 非必要字段
                    },
                    "finish_reason": (choice.get("finish_reason") if isinstance(choice, dict) else getattr(choice, "finish_reason", None)),
                }
            ],
        }

        return JSONResponse(response)

    except Exception as e:
        raise HTTPException(status_code=500, detail=f"Response parsing failed: {e}")


@app.get("/health")
async def health():
    return {"ok": True}


if __name__ == "__main__":
    import uvicorn
    port = int(os.getenv("PORT", 8000))
    uvicorn.run("OpenAIProxyToolCalls:app", host="0.0.0.0", port=port, reload=True)
