import urllib.request
import ssl
import os

ctx = ssl.create_default_context()
ctx.check_hostname = False
ctx.verify_mode = ssl.CERT_NONE

# Try different model URLs from Google MediaPipe
# SSD MobileNetV1 from ONNX Model Zoo
urls_to_try = [
    # YOLOv4 - 目标检测，608x608输入
    "https://github.com/onnx/models/raw/main/validated/vision/object_detection_segmentation/yolov4/yolov4.onnx",
    # Tiny YOLOv2 - 更轻量，416x416输入
    "https://github.com/onnx/models/raw/main/validated/vision/object_detection_segmentation/tiny-yolov2/tiny_yolov2_8.onnx",
]

models_dir = "models"
os.makedirs(models_dir, exist_ok=True)

for url in urls_to_try:
    filename = url.split("/")[-1]
    filepath = os.path.join(models_dir, filename)

    if os.path.exists(filepath):
        print(f"Already exists: {filename}")
        continue

    print(f"Trying to download: {filename}")
    print(f"  URL: {url}")

    try:
        # Create request with headers
        req = urllib.request.Request(url, headers={'User-Agent': 'Mozilla/5.0'})
        response = urllib.request.urlopen(req, timeout=30, context=ctx)

        with open(filepath, 'wb') as f:
            f.write(response.read())

        size = os.path.getsize(filepath)
        print(f"  SUCCESS: {size} bytes")
    except Exception as e:
        print(f"  FAILED: {e}")

print("\nDone!")
