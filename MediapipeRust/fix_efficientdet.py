import onnx
from onnx import shape_inference
import onnxsim

# Try to fix efficientdet_lite0.onnx
model_path = "models/efficientdet_lite0.onnx"
output_path = "models/efficientdet_lite0_fixed.onnx"

print(f"Loading model: {model_path}")
model = onnx.load(model_path)

print("Trying onnxsim to simplify and fix shapes...")
try:
    model_simp, check = onnxsim.simplify(model)
    print(f"Simplified model, check passed: {check}")

    onnx.save(model_simp, output_path)
    print(f"Saved to: {output_path}")
except Exception as e:
    print(f"onnxsim failed: {e}")

    # Try just shape inference
    print("\nTrying shape inference...")
    try:
        model_inferred = shape_inference.infer_shapes(model)
        onnx.save(model_inferred, "models/efficientdet_lite0_inferred.onnx")
        print("Saved with inferred shapes")
    except Exception as e2:
        print(f"Shape inference also failed: {e2}")
