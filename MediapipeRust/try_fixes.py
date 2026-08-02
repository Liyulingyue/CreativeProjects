import onnx
from onnx import shape_inference
import onnxruntime as ort

model_path = "models/efficientdet_lite0.onnx"

# Load model
print("Loading model...")
model = onnx.load(model_path)

print("\n1. Running shape inference with auto_merge=True...")
try:
    inferred_model = shape_inference.infer_shapes(model, auto_merge=True)
    onnx.save(inferred_model, "models/efficientdet_d1.onnx")
    session = ort.InferenceSession("models/efficientdet_d1.onnx")
    print("SUCCESS with auto_merge=True!")
except Exception as e:
    print(f"FAILED: {str(e)[:200]}")

print("\n2. Trying shape inference default mode...")
try:
    inferred_model = shape_inference.infer_shapes(model)
    onnx.save(inferred_model, "models/efficientdet_d2.onnx")
    session = ort.InferenceSession("models/efficientdet_d2.onnx")
    print("SUCCESS!")
except Exception as e:
    print(f"FAILED: {str(e)[:200]}")

print("\n3. Trying onnxsim with dynamic dimensions...")
try:
    import onnxsim
    model_simp, check = onnxsim.simplify(model, dynamic_input_shape=True)
    onnx.save(model_simp, "models/efficientdet_d3.onnx")
    session = ort.InferenceSession("models/efficientdet_d3.onnx")
    print("SUCCESS with onnxsim dynamic!")
except Exception as e:
    print(f"FAILED: {str(e)[:200]}")

print("\n4. Trying onnxsim with skip_shape_inference...")
try:
    import onnxsim
    model_simp, check = onnxsim.simplify(model, skip_shape_inference=True)
    onnx.save(model_simp, "models/efficientdet_d4.onnx")
    session = ort.InferenceSession("models/efficientdet_d4.onnx")
    print("SUCCESS with skip_shape_inference!")
except Exception as e:
    print(f"FAILED: {str(e)[:200]}")
