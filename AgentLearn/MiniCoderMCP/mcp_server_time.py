import asyncio
import datetime
from mcp.server import Server
from mcp.server.models import InitializationOptions
import mcp.types as types
from mcp.server.stdio import stdio_server

# 1. 创建 MCP Server 实例
server = Server("system-time-server")

# 2. 注册获取系统时间的工具
@server.list_tools()
async def handle_list_tools() -> list[types.Tool]:
    """列出可用的工具"""
    return [
        types.Tool(
            name="get_system_time",
            description="获取当前操作系统的本地时间",
            inputSchema={
                "type": "object",
                "properties": {},
            },
        )
    ]

@server.call_tool()
async def handle_call_tool(
    name: str, arguments: dict | None
) -> list[types.TextContent | types.ImageContent | types.EmbeddedResource]:
    """处理工具调用"""
    if name == "get_system_time":
        now = datetime.datetime.now().strftime("%Y-%m-%d %H:%M:%S")
        return [types.TextContent(type="text", text=f"当前系统时间是: {now}")]
    raise ValueError(f"Unknown tool: {name}")

async def main():
    # 3. 通过 stdio 运行服务
    async with stdio_server() as (read, write):
        await server.run(
            read,
            write,
            InitializationOptions(
                server_name="system-time-server",
                server_version="0.1.0",
                capabilities=server.get_capabilities(),
            ),
        )

if __name__ == "__main__":
    asyncio.run(main())
