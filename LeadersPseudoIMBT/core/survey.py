# -*- coding: utf-8 -*-
"""
调研模块
负责加载和管理调研问题数据
"""
import json
import os


def get_questions_with_options():
    """
    从配置文件加载问题和选项数据

    Returns:
        list: 包含(问题文本, 选项列表)元组的列表
    """
    try:
        # 获取当前文件的目录，然后向上查找config目录
        current_dir = os.path.dirname(os.path.abspath(__file__))
        config_dir = os.path.join(os.path.dirname(current_dir), 'config')
        questions_file = os.path.join(config_dir, 'questions.json')

        print(f"📂 正在加载问题文件: {questions_file}")

        if not os.path.exists(questions_file):
            print(f"⚠️ 问题文件不存在: {questions_file}")
            return []

        with open(questions_file, 'r', encoding='utf-8') as f:
            data = json.load(f)

        questions = data.get('questions', [])
        print(f"📊 成功加载 {len(questions)} 个问题")

        # 转换为(问题文本, 选项列表)的格式
        result = []
        for q in questions:
            question_text = q.get('question', '')
            options = q.get('options', [])
            if question_text and options:
                result.append((question_text, options))

        print(f"✅ 转换完成，共 {len(result)} 个有效问题")
        return result

    except Exception as e:
        print(f"❌ 加载问题数据失败: {e}")
        return []


def get_questions_count():
    """
    获取问题总数

    Returns:
        int: 问题数量
    """
    return len(get_questions_with_options())


def get_question_categories():
    """
    获取所有问题类别

    Returns:
        set: 问题类别集合
    """
    try:
        current_dir = os.path.dirname(os.path.abspath(__file__))
        config_dir = os.path.join(os.path.dirname(current_dir), 'config')
        questions_file = os.path.join(config_dir, 'questions.json')

        with open(questions_file, 'r', encoding='utf-8') as f:
            data = json.load(f)

        questions = data.get('questions', [])
        categories = set()
        for q in questions:
            category = q.get('category', '')
            if category:
                categories.add(category)

        return categories

    except Exception as e:
        print(f"❌ 获取问题类别失败: {e}")
        return set()