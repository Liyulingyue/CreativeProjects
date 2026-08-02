import onnxruntime as ort
import numpy as np
import cv2

model_path = "models/ssd_mobilenet_v1.onnx"
img_path = "references/people.png"

print(f"Loading model: {model_path}")
session = ort.InferenceSession(model_path)

inputs = session.get_inputs()
print(f"Inputs: {[(i.name, i.shape, i.type) for i in inputs]}")

outputs = session.get_outputs()
print(f"Outputs: {[(o.name, o.shape) for o in outputs]}")

# Load and preprocess image - uint8 as expected
img = cv2.imread(img_path)
img = cv2.resize(img, (300, 300))
img = img.astype(np.uint8)  # Model expects uint8
img = np.expand_dims(img, axis=0)

print(f"\nInput shape: {img.shape}, dtype: {img.dtype}")

# Run inference
input_name = inputs[0].name
result = session.run(None, {input_name: img})

# boxes: [1, 10, 4] - 10 detections, 4 coordinates (ymin, xmin, ymax, xmax)
# classes: [1, 10] - class indices
# scores: [1, 10] - confidence scores
# num_detections: [1] - actual number of detections

boxes = result[0][0]  # [10, 4]
classes = result[1][0]  # [10]
scores = result[2][0]  # [10]
num_det = int(result[3][0])  # scalar

print(f"\nNum detections: {num_det}")
print(f"\nTop detections:")
for i in range(min(num_det, 10)):
    if scores[i] > 0.3:  # Filter low confidence
        print(f"  class={int(classes[i])}, score={scores[i]:.3f}, box=[ymin={boxes[i][0]:.3f}, xmin={boxes[i][1]:.3f}, ymax={boxes[i][2]:.3f}, xmax={boxes[i][3]:.3f}]")

print("\nSUCCESS!")
