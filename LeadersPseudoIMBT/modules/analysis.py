# -*- coding: utf-8 -*-
import matplotlib.pyplot as plt
import numpy as np
from modules.survey import get_categories
import io
import base64

def calculate_category_scores(answers):
    """
    Calculate average scores for each category
    answers: dict with question id as key, selected option text as value
    """
    categories = get_categories()
    category_scores = {}

    for cat, questions in categories.items():
        scores = []
        for q in questions:
            if q['id'] in answers:
                # Convert selected option text to score (0-4 -> 1-5)
                selected_option = answers[q['id']]
                if selected_option in q['options']:
                    option_index = q['options'].index(selected_option)
                    score = option_index + 1  # Convert to 1-5 scale
                    scores.append(score)
        if scores:
            category_scores[cat] = np.mean(scores)
        else:
            category_scores[cat] = 0

    return category_scores

def generate_radar_chart(scores):
    """
    Generate radar chart and save to file
    """
    categories = list(scores.keys())
    values = list(scores.values())

    # Repeat first value to close the radar chart
    values += values[:1]
    angles = np.linspace(0, 2 * np.pi, len(categories), endpoint=False).tolist()
    angles += angles[:1]

    fig, ax = plt.subplots(figsize=(8, 8), subplot_kw=dict(projection='polar'))
    ax.fill(angles, values, 'b', alpha=0.1)
    ax.plot(angles, values, 'o-', linewidth=2)
    ax.set_xticks(angles[:-1])
    ax.set_xticklabels(categories)
    ax.set_ylim(0, 5)
    ax.set_title('领导特性倾向雷达图', size=16, fontweight='bold', pad=20)
    ax.grid(True)

    # Save to file instead of base64
    import os
    chart_dir = os.path.join(os.path.dirname(__file__), '..', 'charts')
    os.makedirs(chart_dir, exist_ok=True)
    radar_path = os.path.join(chart_dir, 'radar_chart.png')
    fig.savefig(radar_path, format='png', dpi=100, bbox_inches='tight')
    plt.close(fig)

    return radar_path

def generate_bar_chart(scores):
    """
    Generate bar chart and save to file
    """
    categories = list(scores.keys())
    values = list(scores.values())

    fig, ax = plt.subplots(figsize=(12, 6))
    bars = ax.bar(categories, values, color='skyblue', alpha=0.8)
    ax.set_ylabel('平均分数')
    ax.set_title('领导特性倾向柱状图')
    ax.set_ylim(0, 5)
    plt.xticks(rotation=45, ha='right')

    # Add value labels
    for bar in bars:
        height = bar.get_height()
        ax.text(bar.get_x() + bar.get_width()/2., height,
                f'{height:.1f}', ha='center', va='bottom')

    # Save to file instead of base64
    import os
    chart_dir = os.path.join(os.path.dirname(__file__), '..', 'charts')
    os.makedirs(chart_dir, exist_ok=True)
    bar_path = os.path.join(chart_dir, 'bar_chart.png')
    fig.savefig(bar_path, format='png', dpi=100, bbox_inches='tight')
    plt.close(fig)

    return bar_path

def analyze_answers(answers):
    """
    Analyze answers and generate charts
    """
    scores = calculate_category_scores(answers)
    radar_chart = generate_radar_chart(scores)
    bar_chart = generate_bar_chart(scores)

    return scores, radar_chart, bar_chart