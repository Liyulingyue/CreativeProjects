import gradio as gr
from my_rag import chat

def respond(message, history):
    """Handle user message and return response"""
    if not message.strip():
        return "", history

    # Convert Gradio history format to our format
    chat_history = []
    for msg in history:
        chat_history.append({
            "role": msg["role"],
            "content": msg["content"]
        })

    # Get response from our RAG agent
    try:
        response = chat(message, chat_history)
        # Add current exchange to history
        history.append({"role": "user", "content": message})
        history.append({"role": "assistant", "content": response})
        return "", history
    except Exception as e:
        error_msg = f"Error: {str(e)}"
        history.append({"role": "user", "content": message})
        history.append({"role": "assistant", "content": error_msg})
        return "", history

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
        container=True
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