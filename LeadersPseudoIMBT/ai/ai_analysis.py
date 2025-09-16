# -*- coding: utf-8 -*-
"""
AI分析模块
负责与OpenAI API交互，进行领导特性分析
"""
import openai


def generate_ai_analysis(qa_pairs, api_key, base_url, model):
    """
    Generate AI-powered analysis using OpenAI API with pre-formatted qa_pairs
    """
    print("🎯 generate_ai_analysis 函数被调用")
    print(f"📊 问答对数量: {len(qa_pairs)}")

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
        return "⚠️ AI分析服务暂时不可用。\n\n请检查您的API配置，确保包含有效的API_KEY、BASE_URL和MODEL设置。"

    try:
        print("🔗 初始化OpenAI客户端...")
        # Initialize OpenAI client
        client = openai.OpenAI(
            api_key=api_config['api_key'],
            base_url=api_config['base_url']
        )
        print("✅ OpenAI客户端初始化成功")

        # Use the provided qa_pairs directly
        print("📝 使用预处理的问答对数据...")
        qa_text = "\n\n".join(qa_pairs)
        print(f"📋 使用了{len(qa_pairs)}个问答对")
        print(f"📋 准备了{len(qa_pairs)}个问答对")

        print("📝 构建AI提示词...")
        prompt = f"""请基于以下领导特性调研数据进行专业分析：

## 调研数据：
{qa_text}

## 要求：
1. 判断领导类型：狐狸/狼/猫头鹰/兔子/狮子/蜜蜂等等
2. 分析主要特性：工作态度、沟通方式、管理风格、人际关系
3. 提供沟通建议：日常沟通、工作汇报、意见表达、冲突处理

## 输出格式：
领导类型: <修饰词><动物名称>
类型解读: <详细描述>（例如“狡猾的狐狸”，但善于权衡利弊，虽然能力不出众，但能通过灵活应变达成目标；"哈气的猫"，只要你跟他交互，他总是对你态度很差，当他需要你的时候又能和气说话）
交互建议：<具体建议>（例如，少和领导接触，保持最低频率的沟通，汇报工作时突出结果和数据，避免对方打感情牌；适当增加沟通频率，你们可以合作共赢）

上面的输出说明较为简单，你在输出时可以自行加工扩展。沟通建议不应该太以领导为优先，你需要考虑员工是否需要离开部门/离开公司/寻找新的合作机会，这个领导是否适合长期共事。
"""

        print("🚀 正在调用OpenAI API...")
        # Call OpenAI API
        response = client.chat.completions.create(
            model=api_config['model'],
            messages=[
                {"role": "system", "content": "你是一位资深组织行为学专家和领导力教练，擅长通过问卷数据分析领导特性并提供精准的沟通建议。请基于完整的调研数据给出全面、实用的分析。"},
                {"role": "user", "content": prompt}
            ],
            max_tokens=4000,  # 增加token限制
            temperature=0.7
        )
        print("✅ OpenAI API调用成功")

        ai_analysis = response.choices[0].message.content
        print(f"📄 收到的AI分析长度: {len(ai_analysis)}")
        print(f"📄 AI分析结果预览: {ai_analysis[:200]}...")

        # 检查响应是否完整
        if not ai_analysis:
            raise Exception("AI返回的分析结果为空")

        # 检查响应是否被截断
        if ai_analysis.endswith("...") or len(ai_analysis) > 3500:
            print("⚠️ 响应可能被截断，尝试重新生成...")
            # 可以在这里添加重试逻辑

        return ai_analysis

    except Exception as e:
        print(f"AI分析失败: {e}")
        print(f"错误类型: {type(e).__name__}")
        print(f"错误详情: {str(e)}")

        # 提供更友好的错误信息
        error_message = f"⚠️ AI分析服务暂时出现错误：{str(e)}\n\n"

        if "maximum context length" in str(e).lower():
            error_message += "📝 提示：输入内容过长，请尝试减少问题数量或简化回答。\n\n"
        elif "rate limit" in str(e).lower():
            error_message += "⏱️ 提示：API调用频率过高，请稍后再试。\n\n"
        elif "authentication" in str(e).lower():
            error_message += "🔐 提示：API密钥无效，请检查配置。\n\n"
        elif "unterminated string" in str(e).lower():
            error_message += "📄 提示：响应内容过长或格式错误，已自动调整参数重试。\n\n"

        error_message += "如果问题持续存在，请尝试：\n"
        error_message += "1. 检查网络连接\n"
        error_message += "2. 验证API配置\n"
        error_message += "3. 减少输入内容长度\n"
        error_message += "4. 联系技术支持"

        return error_message