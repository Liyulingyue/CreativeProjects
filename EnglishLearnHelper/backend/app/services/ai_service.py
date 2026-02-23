from openai import OpenAI
from dotenv import load_dotenv
import os
import json
import re

load_dotenv()

api_key = os.getenv("MODEL_KEY")
base_url = os.getenv("MODEL_URL", "https://api.openai.com/v1")
model_name = os.getenv("MODEL_NAME", "gpt-4")

print(f"[AI Service] Initialized with model: {model_name}, base_url: {base_url}")

client = OpenAI(api_key=api_key, base_url=base_url)

FORMAT_INSTRUCTIONS = (
    "请仅输出一个 JSON 代码块，严格按照如下格式返回：\n"
    "```json\n"
    "{\n"
    '  "english": "英文短文",\n'
    '  "chinese": "中文翻译"\n'
    "}\n"
    "```\n"
    "其中 english 是英文短文（约500词以内），chinese 是中文翻译。不允许输出除上述 JSON 代码块之外的任何文字。"
)

VOCAB_FORMAT_INSTRUCTIONS = (
    "请仅输出一个 JSON 代码块，严格按照如下格式返回：\n"
    "```json\n"
    "[\n"
    '  {"word": "单词", "phonetic": "音标", "part_of_speech": "词性", "definition": "中文释义"},\n'
    '  {"word": "单词2", "phonetic": "音标2", "part_of_speech": "词性2", "definition": "中文释义2"}\n'
    "]\n"
    "```\n"
    "将识别出的英文单词转换为词汇格式，包含：word(单词)、phonetic(音标)、part_of_speech(词性)、definition(中文释义)。如果没有音标或词性，可以为空字符串。不允许输出除上述 JSON 代码块之外的任何文字。"
)

ARTICLE_SYSTEM_PROMPT = f"""你是一个英语写作助手，擅长用简单的词汇造短文。请根据给出的单词，生成一篇简短的英语短文，要求：
1. 使用尽可能多的给出单词
2. 短文要简单易懂，适合英语学习者
3. 返回 JSON 格式

{VOCAB_FORMAT_INSTRUCTIONS}"""

VOCAB_SYSTEM_PROMPT = f"""你是一个英语词典助手，擅长从文本中提取单词并给出释义。请从给定的英文文本中提取所有单词，并给出音标、词性和中文释义。

{VOCAB_FORMAT_INSTRUCTIONS}"""

JSON_BLOCK_PATTERN = re.compile(r"```json\s*(.*?)\s*```", re.IGNORECASE | re.DOTALL)

def generate_article(words: list[str]) -> dict:
    word_list = ", ".join(words)
    print(f"[AI Service] Generating article for words: {word_list[:50]}...")
    
    response = client.chat.completions.create(
        model=model_name,
        messages=[
            {"role": "system", "content": ARTICLE_SYSTEM_PROMPT},
            {"role": "user", "content": f"请用以下单词造一篇短文：{word_list}"}
        ],
        max_tokens=4096,
        temperature=0.7,
    )
    
    content = response.choices[0].message.content
    
    match = JSON_BLOCK_PATTERN.search(content)
    if match:
        try:
            data = json.loads(match.group(1))
            return {"article": data}
        except json.JSONDecodeError:
            pass
    
    try:
        data = json.loads(content)
        return {"article": data}
    except:
        return {"article": {"english": content, "chinese": ""}}

def convert_to_vocabulary(ocr_texts: list[str]) -> dict:
    text = "\n".join(ocr_texts)
    print(f"[AI Service] Converting OCR to vocabulary, text length: {len(text)}")
    
    response = client.chat.completions.create(
        model=model_name,
        messages=[
            {"role": "system", "content": VOCAB_SYSTEM_PROMPT},
            {"role": "user", "content": f"请从以下文本中提取单词并给出释义：\n{text}"}
        ],
        max_tokens=4096,
        temperature=0.7,
    )
    
    content = response.choices[0].message.content
    
    match = JSON_BLOCK_PATTERN.search(content)
    if match:
        try:
            data = json.loads(match.group(1))
            return {"vocabulary": data}
        except json.JSONDecodeError:
            pass
    
    try:
        data = json.loads(content)
        return {"vocabulary": data}
    except:
        return {"vocabulary": []}
