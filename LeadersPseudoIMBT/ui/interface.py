# -*- coding: utf-8 -*-
"""
用户界面模块
负责Gradio界面的创建和管理
"""
import gradio as gr
from core.survey import get_questions_with_options
from ai.ai_config import load_api_config, validate_and_start
from core.processors import process_answers


def create_interface():
    """
    Create Gradio interface with tabs
    """
    questions = get_questions_with_options()

    # Load saved API configuration
    saved_api_key, saved_base_url, saved_model = load_api_config()

    with gr.Blocks(title="领导特性调研工具", theme=gr.themes.Soft()) as interface:
        with gr.Tabs() as tabs:
            with gr.TabItem("项目介绍", id=0):
                gr.Markdown("# 领导特性调研与分析工具")
                gr.Markdown("""
                ### 项目简介
                本工具基于大模型技术对领导进行智能评价，通过科学的问卷调查分析领导的性格特征、决策风格和管理方式。

                ### 核心功能
                - **领导类型识别**：基于您的回答，系统会为您匹配最相似的领导类型（如狡猾的狐狸、狼群二把手、智慧的猫头鹰等）
                - **沟通建议生成**：根据领导类型，提供个性化的交互策略和沟通技巧
                - **特性分析报告**：生成详细的领导特性分析报告，包括优势、潜在风险和改进建议

                ### 使用说明
                1. 点击"开始测评"进入答题界面
                2. 认真回答30个问题（约5-10分钟）
                3. 查看AI生成的领导类型判断和沟通建议

                ### 注意事项
                - 请根据实际观察和经历选择最符合的选项
                - 系统会基于大模型算法进行智能分析
                - 结果仅供参考，帮助您更好地理解和沟通
                """)

                gr.Markdown("### AI模型配置")
                with gr.Row():
                    api_key_input = gr.Textbox(
                        label="API Key",
                        placeholder="输入您的OpenAI API密钥",
                        type="password",
                        value=saved_api_key
                    )
                    base_url_input = gr.Textbox(
                        label="Base URL",
                        placeholder="https://api.openai.com/v1",
                        value=saved_base_url
                    )
                    model_input = gr.Textbox(
                        label="Model",
                        placeholder="gpt-3.5-turbo",
                        value=saved_model
                    )

                start_btn = gr.Button("开始测评", variant="primary", size="lg")

            with gr.TabItem("答题界面", id=1):
                gr.Markdown("## 请回答下列问题")
                gr.Markdown("请根据您的实际情况选择最符合的选项。")

                inputs = []
                # 横式排版：每行4个问题，从左到右，从上到下
                for i in range(0, len(questions), 4):
                    with gr.Row():
                        for j in range(4):
                            if i + j < len(questions):
                                question, options = questions[i + j]
                                radio = gr.Radio(
                                    label=f"{i+j+1}. {question}",
                                    choices=options,
                                    value=options[0]  # 设置默认值为第一个选项
                                )
                                inputs.append(radio)

                submit_btn = gr.Button("提交分析", variant="primary")

            with gr.TabItem("结果界面", id=2):
                gr.Markdown("## 分析结果")

                analysis_output = gr.Textbox(label="AI分析报告", lines=25)

        # Button actions
        start_btn.click(
            fn=lambda api_key, base_url, model: validate_and_start(api_key, base_url, model),
            inputs=[api_key_input, base_url_input, model_input],
            outputs=[tabs]
        )

        submit_btn.click(
            fn=process_answers,
            inputs=inputs + [api_key_input, base_url_input, model_input],
            outputs=[analysis_output, tabs],
            show_progress=True
        )

        # Clean up old chart files on startup
        import os
        chart_dir = os.path.join(os.path.dirname(__file__), '..', 'charts')
        if os.path.exists(chart_dir):
            for file in os.listdir(chart_dir):
                if file.endswith('.png'):
                    try:
                        os.remove(os.path.join(chart_dir, file))
                    except:
                        pass

    return interface