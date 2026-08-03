from huggingface_hub import hf_hub_download
import os

cache_dir = "models/hf_cache"
os.makedirs(cache_dir, exist_ok=True)

# Try to download SSD MobileNetV1
try:
    filename = hf_hub_download(
        repo_id='onnx-models/ssd_mobilenet_v1',
        filename='ssd_mobilenet_v1.onnx',
        cache_dir=cache_dir
    )
    print(f"Downloaded: {filename}")
except Exception as e:
    print(f"Failed to download ssd_mobilenet_v1: {e}")

# Try Tiny YOLOv2
try:
    filename = hf_hub_download(
        repo_id='onnx-models/tiny_yolov2',
        filename='tiny_yolov2_8.onnx',
        cache_dir=cache_dir
    )
    print(f"Downloaded: {filename}")
except Exception as e:
    print(f"Failed to download tiny_yolov2: {e}")
