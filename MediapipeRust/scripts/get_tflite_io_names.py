import os
import tensorflow as tf
import ssl

ssl._create_default_https_context = lambda: ssl.create_default_context()

models = [
    "deeplab_v3.tflite",
    "blaze_face_short_range.tflite",
]

models_dir = "models"

for model_file in models:
    model_path = os.path.join(models_dir, model_file)
    print(f"\n=== {model_file} ===")

    interpreter = tf.lite.Interpreter(model_path=model_path)
    interpreter.allocate_tensors()

    print("Inputs:")
    for detail in interpreter.get_input_details():
        print(f"  Name: {detail['name']}, Shape: {detail['shape']}, Type: {detail['dtype']}")

    print("Outputs:")
    for detail in interpreter.get_output_details():
        print(f"  Name: {detail['name']}, Shape: {detail['shape']}, Type: {detail['dtype']}")
