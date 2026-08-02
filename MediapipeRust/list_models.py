import urllib.request
import ssl
import re

ctx = ssl.create_default_context()
ctx.check_hostname = False
ctx.verify_mode = ssl.CERT_NONE

url = 'https://storage.googleapis.com/mediapipe-assets/'
response = urllib.request.urlopen(url, timeout=10, context=ctx)
content = response.read().decode('utf-8')

# Find all model files
models = re.findall(r'href="([^"]+\.(tflite|onnx|task))"', content)
for m in models:
    print(m[0])
