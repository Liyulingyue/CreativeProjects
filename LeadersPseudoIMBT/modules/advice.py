# -*- coding: utf-8 -*-
def generate_advice(scores):
    """
    Generate interaction advice based on analysis scores
    scores: dict with category as key, score as value
    """
    advice_list = []

    # Work attitude advice
    if '工作态度' in scores:
        score = scores['工作态度']
        if score < 2:
            advice_list.append("Your leader is very negative about work. Consider taking on more work to gain recognition, but also pay attention to work-life balance.")
        elif score < 3:
            advice_list.append("Your leader has a neutral attitude towards work. Professional work performance is sufficient.")
        elif score < 4:
            advice_list.append("Your leader has a positive attitude towards work. You can appropriately propose innovative ideas, but pay attention to timing.")
        else:
            advice_list.append("Your leader is very focused on work. You can show your work enthusiasm and efficiency, which will get a good response.")

    # Communication style advice
    if '沟通方式' in scores:
        score = scores['沟通方式']
        if score > 3.5:
            advice_list.append("Your leader often uses insulting language. Consider recording important communications and reporting to superiors or seeking HR support.")
        elif score > 2.5:
            advice_list.append("Your leader occasionally uses harsh language. Stay calm and professional in communication.")
        else:
            advice_list.append("Your leader has a gentle communication style. You can freely express your opinions.")

    # Management style advice
    if '管理风格' in scores:
        score = scores['管理风格']
        if score > 3.5:
            advice_list.append("Your leader dominates conversations strongly. Listen first when expressing opinions, then express your views gently.")
        elif score > 2.5:
            advice_list.append("Your leader sometimes dominates conversations. Participate actively in discussions, but don't be too dominant.")
        else:
            advice_list.append("Your leader has a democratic management style. You can actively participate in decision-making.")

    # Decision style advice
    if '决策方式' in scores:
        score = scores['决策方式']
        if score < 2.5:
            advice_list.append("Your leader makes arbitrary decisions. Provide sufficient data to support your suggestions and accept the final decision.")
        elif score < 3.5:
            advice_list.append("Your leader makes consultative decisions. You can provide your opinions before decisions.")
        else:
            advice_list.append("Your leader makes democratic decisions. You can actively participate in team decisions.")

    # General advice
    overall_score = sum(scores.values()) / len(scores) if scores else 0

    if overall_score < 2.5:
        advice_list.append("Overall, your leader may be difficult to get along with. Consider finding a mentor for guidance and looking for career development opportunities.")
    elif overall_score < 3.5:
        advice_list.append("Overall, your leader performs averagely. Being professional is sufficient.")
    else:
        advice_list.append("Overall, your leader is worthy of respect and learning. You can actively seek guidance and cooperation opportunities.")

    # Specific behavioral advice
    advice_list.extend([
        "Pay attention to your leader's body language and tone changes in meetings.",
        "Record important instructions to avoid misunderstandings.",
        "Report progress regularly to maintain transparency.",
        "Seek constructive feedback and continuously improve yourself.",
        "Establish good working relationships but maintain professional boundaries."
    ])

    return advice_list

def get_advice_text(scores):
    """
    Get advice text format
    """
    advice_list = generate_advice(scores)
    return "\n\n".join([f"? {advice}" for advice in advice_list])