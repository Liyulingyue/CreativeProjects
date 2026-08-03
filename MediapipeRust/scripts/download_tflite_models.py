import os
import ssl
import urllib.request

ssl._create_default_https_context = lambda: ssl.create_default_context()

models_to_download = {
    # Image Classifier
    "efficientnet_lite0_fp32.tflite": "https://storage.googleapis.com/mediapipe-models/image_classifier/efficientnet_lite0/float32/1/efficientnet_lite0.tflite",
    # Image Segmenter
    "deeplab_v3.tflite": "https://storage.googleapis.com/mediapipe-models/image_segmenter/deeplab_v3/float32/1/deeplab_v3.tflite",
    # Image Embedder
    "mobilenet_v3_small.tflite": "https://storage.googleapis.com/mediapipe-models/image_embedder/mobilenet_v3_small/float32/1/mobilenet_v3_small.tflite",
    "mobilenet_v3_large.tflite": "https://storage.googleapis.com/mediapipe-models/image_embedder/mobilenet_v3_large/float32/1/mobilenet_v3_large.tflite",
    # Object Detector
    "efficientdet_lite0.tflite": "https://storage.googleapis.com/mediapipe-models/object_detector/efficientdet_lite0/float16/1/efficientdet_lite0.tflite",
    # Face Detector
    "blaze_face_short_range.tflite": "https://storage.googleapis.com/mediapipe-models/face_detector/blaze_face_short_range/float16/1/blaze_face_short_range.tflite",
}

models_dir = "models"

for filename, url in models_to_download.items():
    filepath = os.path.join(models_dir, filename)
    if os.path.exists(filepath):
        print(f"Already exists: {filename}")
        continue
    print(f"Downloading {filename}...")
    try:
        urllib.request.urlretrieve(url, filepath)
        size = os.path.getsize(filepath)
        print(f"  Downloaded: {size} bytes")
    except Exception as e:
        print(f"  Failed: {e}")

print("\nAll downloads complete!")
