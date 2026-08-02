import onnxruntime as ort
import numpy as np
import cv2

model_path = "models/efficientdet_lite0_fixed.onnx"
img_path = "references/people.png"

print(f"Loading model: {model_path}")

try:
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
    print(f"  boxes shape: {result[0].shape}")
    print(f"  classes shape: {result[1].shape}")

    # Find detections
    boxes = result[0][0]
    classes = result[1][0]
    max_scores = classes.max(axis=1)

    threshold = 0.5
    detections = (max_scores > threshold).sum()
    print(f"\nDetections above {threshold}: {detections}")

    if detections > 0:
        top_indices = np.argsort(max_scores)[-detections:][::-1]
        print(f"\nTop {min(5, detections)} detections:")
        for idx in top_indices[:5]:
            cls_idx = np.argmax(classes[idx])
            score = max_scores[idx]
            print(f"  class={cls_idx}, score={score:.3f}")

    print("\nSUCCESS!")

except Exception as e:
    print(f"FAILED: {e}")
    import traceback
    traceback.print_exc()
