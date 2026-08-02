import urllib.request
import ssl
import os

ctx = ssl.create_default_context()
ctx.check_hostname = False
ctx.verify_mode = ssl.CERT_NONE

url = 'https://github.com/ultralytics/assets/releases/download/v8.2.0/yolov8n-pose.onnx'
filepath = 'models/yolov8n-pose.onnx'

if os.path.exists(filepath):
    print(f'Already exists: {filepath}')
else:
    print(f'Downloading {url}...')
    urllib.request.urlretrieve(url, filepath)
    size = os.path.getsize(filepath)
    print(f'Downloaded: {size} bytes')
