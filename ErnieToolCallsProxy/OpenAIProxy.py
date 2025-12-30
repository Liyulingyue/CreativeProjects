#!/usr/bin/env python3
"""OpenAIProxy - 简单的调试代理，用来捕获并记录传入的 OpenAI 风格请求信息

用法：
  python OpenAIProxy.py
或：
  uvicorn OpenAIProxy:app --reload --port 9000

会将每个请求的详细信息打印到 stdout，并追加写入到 ./openai_proxy_requests.log
"""

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