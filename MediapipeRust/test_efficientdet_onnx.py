import onnxruntime as ort
import numpy as np
import cv2

model_path = "models/efficientdet_lite0.onnx"
img_path = "references/people.png"

print(f"Loading model: {model_path}")
session = ort.InferenceSession(model_path)

inputs = session.get_inputs()
print(f"Inputs: {[(i.name, i.shape, i.type) for i in inputs]}")

outputs = session.get_outputs()
print(f"Outputs: {[(o.name, o.shape, o.type) for o in outputs]}")

# Load and preprocess image
img = cv2.imread(img_path)
img = cv2.resize(img, (320, 320))
img = img.astype(np.float32) / 255.0
img = np.expand_dims(img, axis=0)

print(f"\nInput shape: {img.shape}")

# Run inference
input_name = inputs[0].name
result = session.run(None, {input_name: img})

print(f"\nResults:")
print(f"  boxes ( StatefulPartitionedCall:0): shape = {result[0].shape}")
print(f"  classes (StatefulPartitionedCall:1): shape = {result[1].shape}")

# Find detections with scores > threshold
boxes = result[0][0]  # (19206, 4)
classes = result[1][0]  # (19206, 90)

print(f"\nboxes min/max: {boxes.min():.3f} / {boxes.max():.3f}")
print(f"classes min/max: {classes.min():.3f} / {classes.max():.3f}")

# Get max class scores
max_scores = classes.max(axis=1)
print(f"\nMax class scores: min={max_scores.min():.3f}, max={max_scores.max():.3f}")

# Count detections above threshold
threshold = 0.5
detections = (max_scores > threshold).sum()
print(f"Detections above {threshold}: {detections}")

if detections > 0:
    top_indices = np.argsort(max_scores)[-detections:][::-1]
    print(f"\nTop {min(5, detections)} detections:")
    for idx in top_indices[:5]:
        box = boxes[idx]
        cls_idx = np.argmax(classes[idx])
        score = max_scores[idx]
        print(f"  class={cls_idx}, score={score:.3f}, box=[{box[0]:.2f}, {box[1]:.2f}, {box[2]:.2f}, {box[3]:.2f}]")

print("\nSUCCESS!")
