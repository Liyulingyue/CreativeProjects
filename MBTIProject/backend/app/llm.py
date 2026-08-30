"""OpenAI 兼容 Chat Completions 的极简封装，未配置 Key 时上层功能自动降级。"""
import json
import re
from typing import Optional

import httpx

from .config import settings


def llm_enabled() -> bool:
    return bool(settings.llm_api_key)


async def chat(prompt: str, system: str = "你是一名专业的心理测评设计专家。", temperature: float = 0.7) -> str:
    if not llm_enabled():
        raise RuntimeError("LLM 未配置：请在 backend/.env 中设置 LLM_API_KEY")
    async with httpx.AsyncClient(timeout=settings.llm_timeout) as client:
        resp = await client.post(
            f"{settings.llm_base_url.rstrip('/')}/chat/completions",
            headers={"Authorization": f"Bearer {settings.llm_api_key}"},
            json={
                "model": settings.llm_model,
                "messages": [{"role": "system", "content": system}, {"role": "user", "content": prompt}],
                "temperature": temperature,
            },
        )
        resp.raise_for_status()
        return resp.json()["choices"][0]["message"]["content"]


async def chat_json(prompt: str, system: str = "你是一名专业的心理测评设计专家。", temperature: float = 0.5) -> dict:
    """要求模型输出 JSON，并容忍 markdown 代码块包裹。"""
    text = await chat(prompt, system=system, temperature=temperature)
    match = re.search(r"```(?:json)?\s*(.*?)```", text, re.DOTALL)
    if match:
        text = match.group(1)
    start, end = text.find("{"), text.rfind("}")
    if start == -1 or end == -1:
        raise ValueError(f"LLM 未返回 JSON：{text[:200]}")
    return json.loads(text[start:end + 1])
