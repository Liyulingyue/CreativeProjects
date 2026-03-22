"""Simple synchronous HTTP client for the FastAPI MCP server.

Usage:
    client = MCPHttpClient("http://127.0.0.1:8000")
    client.list_tools()
    client.call_tool("list_files", {"path": "."})
"""
import requests
from typing import Any, Dict


class MCPHttpClient:
    def __init__(self, base_url: str = "http://127.0.0.1:8000"):
        self.base = base_url.rstrip("/")

    def list_tools(self):
        resp = requests.get(f"{self.base}/list_tools", timeout=10)
        resp.raise_for_status()
        return resp.json()

    def call_tool(self, name: str, arguments: Dict[str, Any] = None):
        payload = {"name": name, "arguments": arguments or {}}
        resp = requests.post(f"{self.base}/call_tool", json=payload, timeout=60)
        resp.raise_for_status()
        return resp.json()


if __name__ == "__main__":
    # quick manual test
    c = MCPHttpClient()
    print("tools:", c.list_tools())
    print("list_files:", c.call_tool("list_files", {"path": "."}))
