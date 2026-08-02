import tf2onnx
import os

model_path = "models/efficientdet_lite0.tflite"
output_path = "models/efficientdet_lite0.onnx"

input_names = ["serving_default_images:0"]
output_names = ["StatefulPartitionedCall:1", "StatefulPartitionedCall:0"]

print("Trying conversion with different options...")

# Try 1: With extra opset for ai_edge
print("\n=== Try 1: With standard approach ===")
try:
    tf2onnx.convert.from_tflite(
        model_path,
        input_names=input_names,
        output_names=output_names,
        output_path=output_path
    )
    print("SUCCESS!")
except Exception as e:
    print(f"Failed: {e}")

# Try 2: With opset 13
print("\n=== Try 2: With opset 13 ===")
try:
    tf2onnx.convert.from_tflite(
        model_path,
        input_names=input_names,
        output_names=output_names,
        output_path="models/efficientdet_lite0_v2.onnx",
        opset=13
    )
    print("SUCCESS!")
except Exception as e:
    print(f"Failed: {e}")

# Try 3: Check what the actual error is
print("\n=== Try 3: Debug shape inference ===")
try:
    import tensorflow as tf
    import numpy as np

    # Load TFLite
    interpreter = tf.lite.Interpreter(model_path=model_path)
    interpreter.allocate_tensors()

    # Get model signature
    print(f"Model inputs: {[i['name'] for i in interpreter.get_input_details()]}")
    print(f"Model outputs: {[o['name'] for o in interpreter.get_output_details()]}")

    # Check if there are any dynamic shapes
    for i in interpreter.get_input_details():
        print(f"Input {i['name']}: shape = {i['shape']}")

except Exception as e:
    print(f"Debug error: {e}")
