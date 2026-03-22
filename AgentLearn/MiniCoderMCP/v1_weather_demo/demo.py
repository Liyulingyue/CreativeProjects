import asyncio
from mcp import ClientSession, StdioServerParameters
from mcp.client.stdio import stdio_client
from openai import OpenAI

# 1. 配置 MCP Server 的连接参数
# 这里以一个假设的 "weather_server.py" 为例，或者你可以连接现有的 sqlite / filesystem server
server_params = StdioServerParameters(
    command="python",
    args=["path/to/your/weather_server.py"], # 替换为实际的 MCP Server 路径
    env=None
)

async def run_mcp_openai_demo():
    # 初始化 OpenAI 客户端
    openai_client = OpenAI(api_key="your-api-key")

    # 2. 建立 MCP 链接
    async with stdio_client(server_params) as (read, write):
        async with ClientSession(read, write) as session:
            await session.initialize()

            # 3. 列出 MCP Server 提供的工具
            mcp_tools = await session.list_tools()
            
            # 4. 将 MCP 工具转换为 OpenAI 的 Tool 定义格式
            openai_tools = [
                {
                    "type": "function",
                    "function": {
                        "name": tool.name,
                        "description": tool.description,
                        "parameters": tool.inputSchema,
                    },
                }
                for tool in mcp_tools.tools
            ]

            # 5. 用户提问
            messages = [{"role": "user", "content": "帮我查一下旧金山的天气。"}]

            # 6. 第一次调用 OpenAI：让它判断是否需要调用工具
            response = openai_client.chat.completions.create(
                model="gpt-4o",
                messages=messages,
                tools=openai_tools
            )

            tool_call = response.choices[0].message.tool_calls[0]
            
            if tool_call:
                # 7. 执行 MCP 工具
                tool_name = tool_call.function.name
                tool_args = eval(tool_call.function.arguments) # 解析参数

                print(f"正在通过 MCP 调用工具: {tool_name}...")
                result = await session.call_tool(tool_name, tool_args)

                # 8. 将结果传回 OpenAI 得到最终回答
                messages.append(response.choices[0].message)
                messages.append({
                    "role": "tool",
                    "tool_call_id": tool_call.id,
                    "content": str(result.content)
                })

                final_response = openai_client.chat.completions.create(
                    model="gpt-4o",
                    messages=messages
                )
                
                print("最终回答:", final_response.choices[0].message.content)

if __name__ == "__main__":
    asyncio.run(run_mcp_openai_demo())