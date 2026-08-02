import tensorflow as tf
import numpy as np

model_path = "models/pose_extracted/pose_detector.tflite"

print("Loading TFLite model...")
interpreter = tf.lite.Interpreter(model_path=model_path)
interpreter.allocate_tensors()

print("\nInput details:")
for detail in interpreter.get_input_details():
    print(f"  Name: {detail['name']}, Shape: {detail['shape']}, Type: {detail['dtype']}")

print("\nOutput details:")
for detail in interpreter.get_output_details():
    print(f"  Name: {detail['name']}, Shape: {detail['shape']}, Type: {detail['dtype']}")

# Try running inference to see if model works
import cv2
img = cv2.imread("references/people.png")
img = cv2.resize(img, (224, 224))
img = img.astype(np.float32) / 255.0
img = np.expand_dims(img, axis=0)

input_detail = interpreter.get_input_details()[0]
interpreter.set_tensor(input_detail['index'], img)
interpreter.invoke()

print("\nInference succeeded!")
for detail in interpreter.get_output_details():
    output = interpreter.get_tensor(detail['index'])
    print(f"  {detail['name']}: shape={output.shape}, min={output.min():.3f}, max={output.max():.3f}")

# Check if there are any dynamic shapes
print("\nActual output shapes after inference:")
for detail in interpreter.get_output_details():
    output = interpreter.get_tensor(detail['index'])
    print(f"  {detail['name']}: {output.shape}")
