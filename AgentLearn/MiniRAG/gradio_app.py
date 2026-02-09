import gradio as gr
from AgentLearn.MiniRAG.mini_rag import chat_stream
import time

def respond(message, history):
    """Handle user message and return response with streaming"""
    if not message.strip():
        yield "", history
        return

    # Convert Gradio history format to our format
    chat_history = []
    if history:
        for msg in history:
            if isinstance(msg, dict):
                chat_history.append({
                    "role": msg.get("role"),
                    "content": msg.get("content")
                })
            elif isinstance(msg, (list, tuple)) and len(msg) == 2:
                # 支持旧格式 [user_msg, bot_msg]
                if msg[0]:
                    chat_history.append({"role": "user", "content": msg[0]})
                if msg[1]:
                    chat_history.append({"role": "assistant", "content": msg[1]})

    # Add user message to history
    history.append({"role": "user", "content": message})
    
    # Initialize assistant response
    assistant_msg = ""
    process_log = []
    
    # Get streaming response from our RAG agent
    try:
        for chunk in chat_stream(message, chat_history):
            chunk_type = chunk.get("type")
            chunk_content = chunk.get("content", "")
            
            if chunk_type == "thinking":
                process_log.append(f"\n**{chunk_content}**")
            elif chunk_type == "agent_thought":
                process_log.append(f"\n💭 **思考过程:**\n{chunk_content}")
            elif chunk_type == "tool_call":
                tool_name = chunk.get("tool_name", "unknown")
                args = chunk.get("args", {})
                process_log.append(f"\n🔧 **调用工具:** `{tool_name}`\n```json\n{chunk_content.split('参数: ')[1] if '参数: ' in chunk_content else ''}\n```")
            elif chunk_type == "tool_result":
                process_log.append(f"\n📊 **工具结果:**\n```\n{chunk_content.replace('📊 工具结果:', '').strip()}\n```")
            elif chunk_type == "final":
                assistant_msg = chunk_content
            
            # Update display with process log and current answer
            current_display = "\n".join(process_log)
            if assistant_msg:
                current_display += f"\n\n---\n\n✅ **最终答案:**\n{assistant_msg}"
            
            # Yield updated history
            temp_history = history.copy()
            temp_history.append({"role": "assistant", "content": current_display})
            yield "", temp_history
            time.sleep(0.05)  # Small delay for smooth updates
        
        # Final update
        history.append({"role": "assistant", "content": current_display})
        yield "", history
        
    except Exception as e:
        error_msg = f"❌ **错误:** {str(e)}"
        process_log.append(error_msg)
        current_display = "\n".join(process_log)
        history.append({"role": "assistant", "content": current_display})
        yield "", history

def clear_history():
    """Clear chat history"""
    return []

# Create Gradio interface
with gr.Blocks(title="MiniRAG Chat") as demo:
    gr.Markdown("# 🤖 MiniRAG 智能助手")
    gr.Markdown("基于检索增强生成(RAG)的智能对话助手，支持关键词搜索和语义搜索。")

    chatbot = gr.Chatbot(
        height=500,
        show_label=False,
        container=True,
        type="messages"  # 使用messages格式支持流式输出
    )

    with gr.Row():
        msg = gr.Textbox(
            label="输入您的问题",
            placeholder="在这里输入您的问题...",
            scale=4,
            container=False
        )
        submit_btn = gr.Button("发送", scale=1, variant="primary")

    with gr.Row():
        clear_btn = gr.Button("清空对话", variant="secondary")
        gr.Markdown("*提示：支持关键词搜索、语义搜索和迭代检索*")

    # Set up event handlers
    msg.submit(respond, [msg, chatbot], [msg, chatbot])
    submit_btn.click(respond, [msg, chatbot], [msg, chatbot])
    clear_btn.click(clear_history, outputs=[chatbot])

    # Add some examples
    gr.Examples(
        examples=[
            "请解释什么是机器学习",
            "搜索项目中的函数定义",
            "咖啡的起源是什么",
            "如何优化神经网络",
        ],
        inputs=msg,
        label="示例问题"
    )

if __name__ == "__main__":
    demo.launch(
        server_name="0.0.0.0",
        server_port=7860,
        share=False,
        theme=gr.themes.Soft()
    )