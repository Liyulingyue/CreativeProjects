import urllib.request
import ssl
import os

ctx = ssl.create_default_context()
ctx.check_hostname = False
ctx.verify_mode = ssl.CERT_NONE

# Try ultralytics assets
url = 'https://github.com/ultralytics/assets/releases/download/v0.0.0/yolov8n.onnx'
filepath = 'models/yolov8n.onnx'

print(f"Downloading YOLOv8n (nano) ONNX model...")
print(f"URL: {url}")

try:
    req = urllib.request.Request(url, headers={'User-Agent': 'Mozilla/5.0'})
    response = urllib.request.urlopen(req, timeout=60, context=ctx)

    with open(filepath, 'wb') as f:
        f.write(response.read())

    size = os.path.getsize(filepath)
    print(f"SUCCESS: {size} bytes")

    # Check the model
    import onnxruntime as ort
    session = ort.InferenceSession(filepath)
    inputs = session.get_inputs()
    outputs = session.get_outputs()

    print(f"\nModel info:")
    print(f"  Input: {inputs[0].name} {inputs[0].shape}")
    print(f"  Output: {outputs[0].name} {outputs[0].shape}")

except Exception as e:
    print(f"FAILED: {e}")
