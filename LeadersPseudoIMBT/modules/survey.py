# -*- coding: utf-8 -*-
import json
import os

def load_questions():
    """
    Load survey questions from config file
    """
    config_path = os.path.join(os.path.dirname(__file__), '..', 'config', 'questions.json')
    with open(config_path, 'r', encoding='utf-8') as f:
        data = json.load(f)
    return data['questions']

def get_questions_list():
    """
    Get list of all questions for display
    """
    questions = load_questions()
    return [q['question'] for q in questions]

def get_questions_with_options():
    """
    Get questions with their options
    """
    questions = load_questions()
    return [(q['question'], q['options']) for q in questions]

def get_categories():
    """
    Get all question categories
    """
    questions = load_questions()
    categories = {}
    for q in questions:
        cat = q['category']
        if cat not in categories:
            categories[cat] = []
        categories[cat].append(q)
    return categories