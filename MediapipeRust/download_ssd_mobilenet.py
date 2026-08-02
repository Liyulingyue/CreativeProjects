import urllib.request
import ssl
import os
import tensorflow as tf

ctx = ssl.create_default_context()
ctx.check_hostname = False
ctx.verify_mode = ssl.CERT_NONE

url = 'https://storage.googleapis.com/mediapipe-assets/ssd_mobilenet_v1.tflite'
filepath = 'models/ssd_mobilenet_v1.tflite'

if os.path.exists(filepath):
    print(f'Already exists: {filepath}')
else:
    print(f'Downloading {url}...')
    urllib.request.urlretrieve(url, filepath)
    size = os.path.getsize(filepath)
    print(f'Downloaded: {size} bytes')

# Check TFLite input/output
interpreter = tf.lite.Interpreter(model_path=filepath)
interpreter.allocate_tensors()

print('\nTFLite Model:')
for detail in interpreter.get_input_details():
    print(f'  Input: {detail["name"]} shape={detail["shape"]}')

for detail in interpreter.get_output_details():
    print(f'  Output: {detail["name"]} shape={detail["shape"]}')
