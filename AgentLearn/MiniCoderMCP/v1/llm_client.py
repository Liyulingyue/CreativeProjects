#!/usr/bin/env python
"""llm_client.py — small wrapper around the LLM (keeps calls in one place)."""
import os
from typing import List, Dict

MODEL_KEY = os.getenv("MODEL_KEY", "")
BASE_URL = os.getenv("MODEL_URL", "https://api.openai.com/v1")
MODEL_NAME = os.getenv("MODEL_NAME", "gpt-4")


class LLMClient:
    """Thin wrapper that returns the assistant text for a chat-style request.

    - Keeps API calls centralized so agent/tools can remain simple.
    - Avoids importing `openai` at module import time (import inside method).
    """

    def __init__(self, api_key: str = None, base_url: str = None, model_name: str = None):
        self.api_key = api_key or MODEL_KEY
        self.base_url = base_url or BASE_URL
        self.model_name = model_name or MODEL_NAME

    def chat(self, messages: List[Dict], tools: List[Dict] = None, temperature: float = 0.7):
        """Send `messages` to the LLM and return assistant message object or text.

        Arguments:
            messages: chat history to forward to the model
            tools: optional list of tool schemas, each a dict conforming to
                   the OpenAI "function" format.  This list is typically
                   obtained at runtime from an MCP server rather than being
                   hard‑coded.
            temperature: sampling temperature.

        The return value is either a simple string (for errors) or the raw
        message object returned by the OpenAI client; when a tools list is
        provided the caller is responsible for inspecting `message.tool_calls`.
        """
        if not self.api_key:
            return "⚠️  请先设置 MODEL_KEY 环境变量"

        try:
            from openai import OpenAI
            client = OpenAI(api_key=self.api_key, base_url=self.base_url)
            
            params = {
                "model": self.model_name,
                "messages": messages,
                "temperature": temperature,
            }
            if tools:
                params["tools"] = tools

            response = client.chat.completions.create(**params)
            return response.choices[0].message
        except Exception as e:
            return f"⚠️  API调用失败: {str(e)}"
