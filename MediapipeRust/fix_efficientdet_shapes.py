import onnx
from onnx import helper, numpy_helper
import numpy as np

# Load model
model_path = "models/efficientdet_lite0.onnx"
model = onnx.load(model_path)

# Find and fix the problematic shape
# The issue: Resize__534:0 has wrong shape [1,64,4,4] but should be [1,64,5,5]
# because scales are [1,1,1.67,1.67] and input is [1,64,3,3]

fixes_made = []

for val in model.graph.value_info:
    if val.name == "Resize__534:0":
        print(f"Found Resize__534:0")
        if val.type.HasField('tensor_type'):
            shape = val.type.tensor_type.shape.dim
            current_shape = [d.dim_value for d in shape]
            print(f"  Current shape: {current_shape}")
            # Should be [1, 64, 5, 5] based on scales [1,1,1.67,1.67] and input [1,64,3,3]
            # But actually let's check what the TFLite model produces
            shape[2].dim_value = 5  # height
            shape[3].dim_value = 5  # width
            new_shape = [d.dim_value for d in shape]
            print(f"  Fixed shape: {new_shape}")
            fixes_made.append(("Resize__534:0", current_shape, new_shape))

# Also need to check the output of the Add node that consumes this
for val in model.graph.value_info:
    if "op_after_combine5/Relu6" in val.name and "add_n" in val.name:
        print(f"\nFound Add output: {val.name}")
        if val.type.HasField('tensor_type'):
            shape = val.type.tensor_type.shape.dim
            current_shape = [d.dim_value for d in shape]
            print(f"  Current shape: {current_shape}")
            # This should also be [1, 64, 5, 5] to match
            if current_shape == [1, 64, 4, 4]:
                shape[2].dim_value = 5
                shape[3].dim_value = 5
                new_shape = [d.dim_value for d in shape]
                print(f"  Fixed shape: {new_shape}")
                fixes_made.append((val.name, current_shape, new_shape))

print(f"\n\nTotal fixes made: {len(fixes_made)}")

if fixes_made:
    # Save the fixed model
    fixed_path = "models/efficientdet_lite0_fixed_v2.onnx"
    onnx.save(model, fixed_path)
    print(f"Saved fixed model to: {fixed_path}")

    # Try to load with ONNX Runtime
    print("\nTrying to load fixed model with ONNX Runtime...")
    import onnxruntime as ort
    try:
        session = ort.InferenceSession(fixed_path)
        print("SUCCESS!")
    except Exception as e:
        print(f"FAILED: {e}")
