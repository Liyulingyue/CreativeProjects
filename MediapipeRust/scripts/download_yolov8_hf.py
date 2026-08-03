from huggingface_hub import hf_hub_download
import os

cache_dir = "models/hf_cache"
os.makedirs(cache_dir, exist_ok=True)

# YOLOv8 nano - 最小最快的版本
# Ultralytics YOLOv8 AGPL-3.0 许可证
print("Downloading YOLOv8n (nano) from HuggingFace...")
print("License: AGPL-3.0 (Ultralytics)")

try:
    # YOLOv8n-pose for pose detection
    filename = hf_hub_download(
        repo_id='stevenAG/YOLOv8n-ONNX',
        filename='yolov8n-pose.onnx',
        cache_dir=cache_dir
    )
    print(f"Downloaded: {filename}")

    # Check model info
    import onnxruntime as ort
    session = ort.InferenceSession(filename)
    inputs = session.get_inputs()
    outputs = session.get_outputs()

    print(f"\nModel info:")
    print(f"  Input: {inputs[0].name} {inputs[0].shape}")
    for i, out in enumerate(outputs):
        print(f"  Output {i}: {out.name} {out.shape}")

except Exception as e:
    print(f"Failed: {e}")

# Also try the detection model
print("\n\nTrying YOLOv8n detection model...")
try:
    filename = hf_hub_download(
        repo_id='stevenAG/YOLOv8n-ONNX',
        filename='yolov8n.onnx',
        cache_dir=cache_dir
    )
    print(f"Downloaded: {filename}")

    import onnxruntime as ort
    session = ort.InferenceSession(filename)
    inputs = session.get_inputs()
    outputs = session.get_outputs()

    print(f"\nModel info:")
    print(f"  Input: {inputs[0].name} {inputs[0].shape}")
    for i, out in enumerate(outputs):
        print(f"  Output {i}: {out.name} {out.shape}")

except Exception as e:
    print(f"Failed: {e}")
