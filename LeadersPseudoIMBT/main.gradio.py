# -*- coding: utf-8 -*-
import gradio as gr
from modules.survey import get_questions_with_options
from modules.advice import get_advice_text
import openai
import json

# Global variable to store API configuration
current_api_config = {}

def generate_ai_analysis(questions, answers_dict, api_key, base_url, model):
    """
    Generate AI-powered analysis using OpenAI API with all questions and answers
    """
    print("🎯 generate_ai_analysis 函数被调用")
    print(f"📊 问题数量: {len(questions)}")
    print(f"📝 答案数量: {len(answers_dict)}")

    # Use the provided API configuration instead of global variable
    api_config = {
        'api_key': api_key,
        'base_url': base_url,
        'model': model
    }

    print(f"🔧 当前API配置: {api_config}")

    # Check if API configuration is available
    if not api_config.get('api_key') or not api_config.get('base_url') or not api_config.get('model'):
        print("⚠️ API配置不完整，使用fallback模式")
        # Fallback to basic analysis without traditional scoring
        return "⚠️ AI分析服务暂时不可用。\n\n请检查您的API配置（.env文件），确保包含有效的API_KEY、BASE_URL和MODEL设置。\n\n您可以通过以下方式配置：\n1. 复制 .env.example 为 .env\n2. 填入您的OpenAI API密钥\n3. 设置正确的API地址和模型名称"

    try:
        print("🔗 初始化OpenAI客户端...")
        # Initialize OpenAI client
        client = openai.OpenAI(
            api_key=api_config['api_key'],
            base_url=api_config['base_url']
        )
        print("✅ OpenAI客户端初始化成功")

        # Prepare all questions and answers
        print("📝 准备问题和答案数据...")
        qa_pairs = []
        for i, (question, options) in enumerate(questions):
            q_id = i + 1
            if q_id in answers_dict:
                answer = answers_dict[q_id]
                qa_pairs.append(f"问题{q_id}: {question}\n回答: {answer}")

        qa_text = "\n\n".join(qa_pairs)
        print(f"📋 准备了{len(qa_pairs)}个问答对")

        print("📝 构建AI提示词...")
        prompt = f"""基于以下领导特性调研的所有问题和答案，请为用户提供专业的领导类型分析和沟通建议。

## 调研问答详情：
{qa_text}

## 分析要求：
请基于以上所有问题和答案，全面分析这位领导的特性，按照以下格式输出分析结果：

### 🦊 领导类型判断
[根据所有回答判断领导属于哪种动物类型，并给出判断依据]
- 狡猾的狐狸：精明、策略性强、注重利益
- 狼群二把手：强势、竞争性、团队领导力
- 智慧的猫头鹰：理性、分析力强、注重细节
- 温和的兔子：温和、包容性、注重和谐
- 勇猛的狮子：自信、决策力强、领导魅力
- 勤劳的蜜蜂：勤奋、责任心强、注重效率

### 📊 综合特性分析
详细分析领导的主要特性：
- 工作态度：积极性、责任心、执行力
- 沟通方式：表达风格、倾听能力、反馈方式
- 管理风格：决策方式、团队管理、授权程度
- 人际关系：同事相处、上下级关系、冲突处理
- 领导魅力：影响力、激励方式、团队凝聚力

### 💡 个性化沟通建议
基于以上分析，提供具体的沟通策略：
1. **日常沟通**：最佳沟通时机、方式和话题选择
2. **工作汇报**：汇报内容组织、时机把握、重点突出
3. **意见表达**：提出建议的方式、时机选择、说服技巧
4. **冲突处理**：面对分歧时的应对策略、化解方法
5. **职业发展**：如何争取机会、展现能力、建立关系
6. **注意事项**：需要避免的行为、潜在风险、改进方向

请用专业、建设性的语言输出，确保分析客观准确，建议实用可行。"""

        print("🚀 正在调用OpenAI API...")
        # Call OpenAI API
        response = client.chat.completions.create(
            model=api_config['model'],
            messages=[
                {"role": "system", "content": "你是一位资深组织行为学专家和领导力教练，擅长通过问卷数据分析领导特性并提供精准的沟通建议。请基于完整的调研数据给出全面、实用的分析。"},
                {"role": "user", "content": prompt}
            ],
            max_tokens=16000,
            temperature=0.7
        )
        print("✅ OpenAI API调用成功")

        ai_analysis = response.choices[0].message.content.strip()
        print(f"📄 收到的AI分析长度: {len(ai_analysis)}")
        return ai_analysis

    except Exception as e:
        print(f"AI分析失败: {e}")
        # Fallback to basic analysis without traditional scoring
        return f"⚠️ AI分析服务暂时出现错误：{str(e)}\n\n请检查您的网络连接和API配置。\n\n如果问题持续存在，请尝试：\n1. 验证API密钥是否有效\n2. 检查网络连接\n3. 确认使用的模型是否可用"

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

    # Convert answers to dictionary format
    answers_dict = {}
    questions = get_questions_with_options()
    print(f"📋 加载的问题数量: {len(questions)}")

    for i, answer in enumerate(answers):
        if i < len(questions):
            answers_dict[i + 1] = answer  # Question ID starts from 1
            print(f"✅ 问题{i+1}: {answer}")
        else:
            print(f"⚠️ 额外答案{i}: {answer}")

    print(f"📝 答案字典内容: {answers_dict}")

    # 首先返回加载状态和跳转到结果页面
    loading_message = """🤖 AI分析进行中...

⏳ 正在分析您的回答...
⏳ 正在生成领导类型判断...
⏳ 正在准备个性化沟通建议...

请稍候，分析需要10-30秒...

💡 提示：分析完成后将自动显示完整报告"""
    yield loading_message, gr.update(selected=2)

    # Generate AI-powered analysis with all questions and answers
    print("🤖 开始生成AI分析...")
    print("📊 正在准备数据...")
    analysis_result = generate_ai_analysis(questions, answers_dict, api_key, base_url, model)
    print(f"📄 AI分析结果长度: {len(analysis_result)}")
    print(f"📄 AI分析结果预览: {analysis_result[:200]}...")

    print("✅ process_answers 函数执行完成")
    yield analysis_result, gr.update(selected=2)

def validate_and_start(api_key, base_url, model):
    """
    Validate API configuration and start survey (optional)
    """
    # Store configuration for later use (even if empty)
    global current_api_config
    current_api_config = {
        'api_key': api_key,
        'base_url': base_url,
        'model': model
    }

    # Always proceed to survey tab
    return gr.update(selected=1)

def load_api_config():
    """
    Load API configuration from .env file
    """
    try:
        import os
        from dotenv import load_dotenv

        # Load .env file
        load_dotenv()

        api_key = os.getenv('API_KEY', '')
        base_url = os.getenv('BASE_URL', '')
        model = os.getenv('MODEL', '')

        return api_key, base_url, model
    except ImportError:
        # If python-dotenv is not installed, try to read .env file manually
        try:
            import os
            env_file = os.path.join(os.path.dirname(__file__), '.env')

            if os.path.exists(env_file):
                config = {}
                with open(env_file, 'r', encoding='utf-8') as f:
                    for line in f:
                        line = line.strip()
                        if line and not line.startswith('#'):
                            key, value = line.split('=', 1)
                            config[key.strip()] = value.strip()

                return (
                    config.get('API_KEY', ''),
                    config.get('BASE_URL', ''),
                    config.get('MODEL', '')
                )
            else:
                return '', '', ''
        except Exception as e:
            return '', '', ''
    except Exception as e:
        return '', '', ''

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
                        placeholder="输入您的API密钥",
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
    print("Starting Leader Characteristics Survey Tool...")
    interface = create_interface()
    print("Interface created successfully. Launching server...")
    interface.launch(server_name="0.0.0.0", server_port=7861, show_error=True, share=False)
