# -*- coding: utf-8 -*-
import gradio as gr
from modules.survey import get_questions_with_options
from modules.analysis import analyze_answers
from modules.advice import get_advice_text

def process_answers(*answers):
    """
    Process user answers and generate analysis results
    """
    # Convert answers to dictionary format
    answers_dict = {}
    questions = get_questions_with_options()
    for i, answer in enumerate(answers):
        if i < len(questions):
            answers_dict[i + 1] = answer  # Question ID starts from 1

    # Analyze answers
    scores, radar_chart, bar_chart = analyze_answers(answers_dict)

    # Generate advice
    advice = get_advice_text(scores)

    # Format score display
    scores_text = "\n".join([f"{cat}: {score:.1f}" for cat, score in scores.items()])

    return scores_text, radar_chart, bar_chart, advice

def create_interface():
    """
    Create Gradio interface
    """
    questions = get_questions_with_options()

    with gr.Blocks(title="领导特性调研工具", theme=gr.themes.Soft()) as interface:
        gr.Markdown("# 领导特性调研与分析工具")
        gr.Markdown("请回答以下30个问题，帮助分析领导的特性倾向。")

        inputs = []
        with gr.Row():
            with gr.Column(scale=1):
                for i, (question, options) in enumerate(questions[:15]):
                    radio = gr.Radio(
                        label=f"{i+1}. {question}",
                        choices=options,
                        value=None
                    )
                    inputs.append(radio)

            with gr.Column(scale=1):
                for i, (question, options) in enumerate(questions[15:], start=15):
                    radio = gr.Radio(
                        label=f"{i+1}. {question}",
                        choices=options,
                        value=None
                    )
                    inputs.append(radio)

        submit_btn = gr.Button("提交分析", variant="primary")

        with gr.Row():
            with gr.Column():
                scores_output = gr.Textbox(label="特性分数", lines=10)
                advice_output = gr.Textbox(label="交互建议", lines=15)

            with gr.Column():
                radar_output = gr.Image(label="雷达图")
                bar_output = gr.Image(label="柱状图")

        submit_btn.click(
            fn=process_answers,
            inputs=inputs,
            outputs=[scores_output, radar_output, bar_output, advice_output]
        )

        # Clean up old chart files on startup
        import os
        chart_dir = os.path.join(os.path.dirname(__file__), 'charts')
        if os.path.exists(chart_dir):
            for file in os.listdir(chart_dir):
                if file.endswith('.png'):
                    try:
                        os.remove(os.path.join(chart_dir, file))
                    except:
                        pass

    return interface

if __name__ == "__main__":
    interface = create_interface()
    interface.launch(server_name="0.0.0.0", server_port=7860)