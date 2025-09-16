# -*- coding: utf-8 -*-
"""
核心处理器模块
负责用户答案的处理和业务逻辑
"""
from core.survey import get_questions_with_options
from ai.ai_analysis import generate_ai_analysis


def process_answers(*args):
    """
    Process user answers and generate analysis results with loading state
    """
    print("🔍 process_answers 函数被调用")
    print(f"📊 收到的参数数量: {len(args)}")

    # 最后三个参数是API配置，前面的都是答案
    num_answers = len(args) - 3
    answers = args[:num_answers]
    api_key, base_url, model = args[-3:]

    print(f"📊 答案数量: {len(answers)}")
    print(f"🔧 API配置: key={api_key[:10]}..., url={base_url}, model={model}")

    # Convert answers to qa_pairs format directly
    qa_pairs = []
    questions = get_questions_with_options()
    print(f"📋 加载的问题数量: {len(questions)}")

    for i, answer in enumerate(answers):
        if i < len(questions):
            question, options = questions[i]
            qa_pairs.append(f"问题: {question}\n回答: {answer}")

    print(f"📝 组织了{len(qa_pairs)}个问答对")

    # 首先返回加载状态和跳转到结果页面
    loading_message = """🤖 AI分析进行中...

⏳ 正在分析您的回答...
⏳ 正在生成领导类型判断...
⏳ 正在准备个性化沟通建议...

请稍候，分析需要10-30秒...

💡 提示：分析完成后将自动显示完整报告"""
    import gradio as gr
    yield loading_message, gr.update(selected=2)

    # Generate AI-powered analysis with qa_pairs directly
    print("🤖 开始生成AI分析...")
    print("📊 正在准备数据...")
    analysis_result = generate_ai_analysis(qa_pairs, api_key, base_url, model)
    print(f"📄 AI分析结果长度: {len(analysis_result)}")
    print(f"📄 AI分析结果预览: {analysis_result[:200]}...")

    print("✅ process_answers 函数执行完成")
    yield analysis_result, gr.update(selected=2)