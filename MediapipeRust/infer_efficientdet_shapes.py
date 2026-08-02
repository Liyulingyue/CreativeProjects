import onnx
from onnx import shape_inference
import onnxruntime as ort

model_path = "models/efficientdet_lite0.onnx"

# Load and try shape inference
print("Loading model...")
model = onnx.load(model_path)

print("Running shape inference...")
try:
    # Try onnx shape inference
    model_inferred = shape_inference.infer_shapes(model)

    # Save the inferred model
    inferred_path = "models/efficientdet_lite0_inferred.onnx"
    onnx.save(model_inferred, inferred_path)
    print(f"Saved inferred model to: {inferred_path}")

    # Try to load with ONNX Runtime
    print("\nTrying to load inferred model with ONNX Runtime...")
    session = ort.InferenceSession(inferred_path)
    print("SUCCESS! ONNX Runtime loaded the model!")

    inputs = session.get_inputs()
    outputs = session.get_outputs()
    print(f"\nInputs: {[(i.name, i.shape) for i in inputs]}")
    print(f"Outputs: {[(o.name, o.shape) for o in outputs]}")

except Exception as e:
    print(f"FAILED: {e}")
    import traceback
    traceback.print_exc()
