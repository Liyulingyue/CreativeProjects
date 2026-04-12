from openai import OpenAI
from dotenv import load_dotenv
import os
import json

load_dotenv()

api_key = os.getenv("MODEL_KEY")
base_url = os.getenv("MODEL_URL", "https://api.openai.com/v1")
model_name = os.getenv("MODEL_NAME", "gpt-4")

print(f"[AI Service] Initialized with model: {model_name}, base_url: {base_url}")

client = OpenAI(api_key=api_key, base_url=base_url)

ARTICLE_TARGET = {
    "english": "string (英文短文，约500词以内，使用给出的单词)",
    "chinese": "string (中文翻译)"
}

VOCAB_TARGET = [
    {
        "word": "string (英文单词)",
        "phonetic": "string (音标，可为空)",
        "part_of_speech": "string (词性，可为空)",
        "definition": "string (中文释义)"
    }
]

article_wrapper = None
vocab_wrapper = None

def _get_article_wrapper():
    global article_wrapper
    if article_wrapper is None:
        from OpenAIJsonWrapper import OpenAIJsonWrapper
        article_wrapper = OpenAIJsonWrapper(
            client,
            model=model_name,
            target_structure=ARTICLE_TARGET,
            background="你是一个英语写作助手，擅长用简单的词汇造短文。请根据给出的单词，生成一篇简短的英语短文，要求：1. 使用尽可能多的给出单词；2. 短文要简单易懂，适合英语学习者。"
        )
    return article_wrapper

def _get_vocab_wrapper():
    global vocab_wrapper
    if vocab_wrapper is None:
        from OpenAIJsonWrapper import OpenAIJsonWrapper
        vocab_wrapper = OpenAIJsonWrapper(
            client,
            model=model_name,
            target_structure=VOCAB_TARGET
        )
    return vocab_wrapper

def reload_config() -> None:
    global api_key, base_url, model_name, client, article_wrapper, vocab_wrapper
    api_key = os.getenv("MODEL_KEY")
    base_url = os.getenv("MODEL_URL", "https://api.openai.com/v1")
    model_name = os.getenv("MODEL_NAME", "gpt-4")
    client = OpenAI(api_key=api_key, base_url=base_url)
    article_wrapper = None
    vocab_wrapper = None
    print(f"[AI Service] Reloaded with model: {model_name}, base_url: {base_url}")

def generate_article(words: list[str]) -> dict:
    word_list = ", ".join(words)
    print(f"[AI Service] Generating article for words: {word_list[:50]}...")

    wrapper = _get_article_wrapper()
    result = wrapper.chat(
        messages=[{"role": "user", "content": f"请用以下单词造一篇短文：{word_list}"}],
        extra_requirements=[
            "英文短文约500词以内",
            "使用尽可能多的给出单词",
            "短文要简单易懂，适合英语学习者"
        ]
    )

    if result["error"]:
        print(f"[AI Service] Article generation error: {result['error']}")
        return {"article": {"english": result["raw_content"] or "", "chinese": ""}}

    return {"article": result["data"]}

def convert_to_vocabulary(ocr_texts: list[str]) -> dict:
    text = "\n".join(ocr_texts)
    print(f"[AI Service] Converting OCR to vocabulary, text length: {len(text)}")

    wrapper = _get_vocab_wrapper()
    result = wrapper.chat(
        messages=[{"role": "user", "content": f"请从以下文本中提取单词并给出释义：\n{text}"}],
        extra_requirements=[
            "包含 word(单词)、phonetic(音标)、part_of_speech(词性)、definition(中文释义)",
            "如果没有音标或词性，可以为空字符串"
        ]
    )

    if result["error"]:
        print(f"[AI Service] Vocab conversion error: {result['error']}")
        return {"vocabulary": []}

    data = result["data"]
    if isinstance(data, list):
        return {"vocabulary": data}
    return {"vocabulary": []}
