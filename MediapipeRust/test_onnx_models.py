import onnxruntime as ort
import numpy as np
import cv2
import os

models_dir = "models"
reference_dir = "references"

models_info = {
    "efficientnet_lite0_fp32.onnx": {
        "input": [1, 224, 224, 3],  # NHWC
        "task": "image_classification",
    },
    "deeplab_v3.onnx": {
        "input": [1, 257, 257, 3],  # NHWC
        "task": "image_segmentation",
    },
    "mobilenet_v3_small.onnx": {
        "input": [1, 224, 224, 3],  # NHWC
        "task": "image_embedding",
    },
    "mobilenet_v3_large.onnx": {
        "input": [1, 224, 224, 3],  # NHWC
        "task": "image_embedding",
    },
    "efficientdet_lite0.onnx": {
        "input": [1, 320, 320, 3],  # NHWC
        "task": "object_detection",
    },
    "blaze_face_short_range.onnx": {
        "input": [1, 128, 128, 3],  # NHWC
        "task": "face_detection",
    },
}

test_images = {
    "efficientnet_lite0_fp32.onnx": "cat.png",
    "deeplab_v3.onnx": "cat.png",
    "mobilenet_v3_small.onnx": "cat.png",
    "mobilenet_v3_large.onnx": "cat.png",
    "efficientdet_lite0.onnx": "people.png",
    "blaze_face_short_range.onnx": "hand.png",
}

sess_options = ort.SessionOptions()
sess_options.graph_optimization_level = ort.GraphOptimizationLevel.ORT_ENABLE_ALL

for model_file, info in models_info.items():
    model_path = os.path.join(models_dir, model_file)
    print(f"\n=== Testing {model_file} ===")
    print(f"  Task: {info['task']}")
    print(f"  Expected input shape: {info['input']}")

    if not os.path.exists(model_path):
        print(f"  ERROR: Model not found!")
        continue

    try:
        session = ort.InferenceSession(model_path, sess_options)

        inputs = session.get_inputs()
        print(f"  Model inputs: {[(i.name, i.shape, i.type) for i in inputs]}")

        outputs = session.get_outputs()
        print(f"  Model outputs: {[(o.name, o.shape, o.type) for o in outputs]}")

        # Load and preprocess image
        img_name = test_images.get(model_file, "cat.png")
        img_path = os.path.join(reference_dir, img_name)
        img = cv2.imread(img_path)
        if img is None:
            print(f"  ERROR: Image not found: {img_path}")
            continue

        # Get expected input size from model
        input_shape = inputs[0].shape  # e.g., [1, 224, 224, 3]
        h, w = input_shape[1], input_shape[2]

        # Resize to model's expected size (keep NHWC format, no transpose)
        img = cv2.resize(img, (w, h))
        img = img.astype(np.float32) / 255.0
        img = np.expand_dims(img, axis=0)  # Add batch dimension

        print(f"  Input image shape: {img.shape}")

        # Run inference
        input_name = inputs[0].name
        result = session.run(None, {input_name: img})

        if "efficientnet" in model_file:
            probs = result[0][0]
            top5 = np.argsort(probs)[-5:][::-1]
            print(f"  Top 5 classes: {top5}")
            print(f"  Top 5 probs: {probs[top5]}")
        elif "mobilenet" in model_file:
            embedding = result[0][0]
            print(f"  Embedding dim: {len(embedding)}")
            print(f"  First 5 values: {embedding[:5]}")
        elif "deeplab" in model_file:
            mask = result[0][0]
            print(f"  Segmentation mask shape: {mask.shape}")
            unique_classes = np.unique(np.argmax(mask, axis=-1))
            print(f"  Unique classes: {unique_classes}")
        elif "efficientdet" in model_file:
            boxes = result[0]
            print(f"  Detection boxes shape: {boxes.shape}")
        elif "blaze_face" in model_file:
            regressors, classificators = result
            print(f"  Regressors shape: {regressors.shape}")
            print(f"  Classificators shape: {classificators.shape}")

        print(f"  SUCCESS!")

    except Exception as e:
        print(f"  ERROR: {e}")
        import traceback
        traceback.print_exc()

print("\n=== All tests complete! ===")
