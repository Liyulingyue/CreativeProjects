from openai import OpenAI
import time
import os
from dotenv import load_dotenv

# 如果存在 .env 文件，则加载环境变量
if os.path.exists('.env'):
    load_dotenv()

MODEL_URL = os.getenv('MODEL_URL', 'YOUR_MODEL_URL_HERE')
MODEL_KEY = os.getenv('MODEL_KEY', 'YOUR_API_KEY_HERE')
MODEL_NAME = os.getenv('MODEL_NAME', 'YOUR_MODEL_NAME_HERE')

# 初始化客户端
client = OpenAI(
    base_url=MODEL_URL,
    api_key=MODEL_KEY
)

time1 = time.time()
response = client.chat.completions.create(
    model=MODEL_NAME, # 指定模型
    messages=[
        {"role": "system", "content": "you are a helpful assistant that speaks Chinese."},
        {"role": "user", "content": "hi? Introduce yourself in Chinese."}
    ],
    temperature=0.7,  # 控制生成多样性
    max_tokens=512    # 最大生成 token 数
)
time2 = time.time()

# 打印结果
print(response.choices[0].message.content)

print(f"耗时：{time2 - time1}秒")
print(f"回复速率：{ len(response.choices[0].message.content) / (time2 - time1) } 字符/秒")
