import onnxruntime as ort
import numpy as np
import cv2

model_path = "models/pose_landmarks_detector.onnx"
img_path = "references/people.png"

print(f"Loading model: {model_path}")
session = ort.InferenceSession(model_path)

inputs = session.get_inputs()
print(f"Inputs: {[(i.name, i.shape, i.type) for i in inputs]}")

outputs = session.get_outputs()
print(f"Outputs: {[(o.name, o.shape, o.type) for o in outputs]}")

# Load and preprocess image
img = cv2.imread(img_path)
img = cv2.resize(img, (256, 256))
img = img.astype(np.float32) / 255.0
img = np.expand_dims(img, axis=0)

print(f"\nInput shape: {img.shape}")

# Run inference
input_name = inputs[0].name
result = session.run(None, {input_name: img})

print(f"\nResults:")
for i, r in enumerate(result):
    print(f"  Output {i}: shape = {r.shape}, dtype = {r.dtype}")

# Pose landmarks: result[0] should be the landmark coordinates
# 195 values = 33 landmarks * 3 (x, y, z) or 33 * 5 (x, y, z, visibility, presence)
landmarks = result[0][0]  # [195]
print(f"\nLandmarks (first 10): {landmarks[:10]}")
print(f"Landmarks shape: {landmarks.shape}")

# Check if result looks like valid landmark data
num_landmarks = 33
if len(landmarks) == num_landmarks * 3:
    print(f"Detected 33 landmarks in 3D (x, y, z) format")
elif len(landmarks) == num_landmarks * 5:
    print(f"Detected 33 landmarks with visibility/presence (x, y, z, v, p)")
    landmarks_reshaped = landmarks.reshape(33, 5)
    print(f"First landmark: {landmarks_reshaped[0]}")

print("\nSUCCESS!")
