import onnx
from onnx import helper

# Load model
model_path = "models/efficientdet_lite0.onnx"
model = onnx.load(model_path)

# First let's run shape inference to get as many shapes as possible
print("Running initial shape inference...")
from onnx import shape_inference
model = shape_inference.infer_shapes(model)

# Now find the problematic Add node and trace back to find actual shapes
# from nodes that DO have shape info

# Let's find all nodes that have shape info for their outputs
shapes_known = {}
for val in model.graph.value_info:
    if val.type.HasField('tensor_type'):
        shape = tuple(d.dim_value if d.dim_value > 0 else 0 for d in val.type.tensor_type.shape.dim)
        if 0 not in shape:  # Only keep fully known shapes
            shapes_known[val.name] = shape

print(f"Found {len(shapes_known)} tensors with known shapes")

# Find the inputs to the problematic Add
problematic_add_inputs = [
    'resample_p6/max_pooling2d/MaxPool',
    'Resize__534:0'
]

print(f"\nChecking problematic inputs:")
for inp in problematic_add_inputs:
    if inp in shapes_known:
        print(f"  {inp}: {shapes_known[inp]}")
    else:
        print(f"  {inp}: UNKNOWN")

# Now let's check what the Resize node's input shape is
# Resize__534 takes: resample_p7/max_pooling2d_1/MaxPool as input
# and scales__747 = [1, 1, 1.67, 1.67]

# Find resample_p7 input
for node in model.graph.node:
    if node.name == "Resize__534":
        print(f"\nResize__534 inputs: {list(node.input)}")
        for inp in node.input:
            if inp in shapes_known:
                print(f"  {inp}: {shapes_known[inp]}")
            else:
                print(f"  {inp}: UNKNOWN")

# Based on TFLite conversion, resample_p7/max_pooling2d_1/MaxPool output is [1, 64, 3, 3]
# With scales [1,1,1.67,1.67], the output should be [1, 64, 5, 5]
# But we need to add this info to the model

print("\n\nAttempting to add missing value_info entries...")

# Add value_info for resample_p6/max_pooling2d/MaxPool
# It should be [1, 64, 3, 3] based on the graph structure
val_info_1 = helper.make_tensor_value_info(
    'resample_p6/max_pooling2d/MaxPool',
    onnx.TensorProto.FLOAT,
    [1, 64, 3, 3]
)

# Add value_info for Resize__534:0
# It should be [1, 64, 5, 5] based on scales [1,1,1.67,1.67] applied to [1,64,3,3]
val_info_2 = helper.make_tensor_value_info(
    'Resize__534:0',
    onnx.TensorProto.FLOAT,
    [1, 64, 5, 5]
)

# Add to graph
model.graph.value_info.append(val_info_1)
model.graph.value_info.append(val_info_2)

# Save
fixed_path = "models/efficientdet_lite0_manually_fixed.onnx"
onnx.save(model, fixed_path)
print(f"Saved to: {fixed_path}")

# Test with ONNX Runtime
print("\nTesting with ONNX Runtime...")
import onnxruntime as ort
try:
    session = ort.InferenceSession(fixed_path)
    print("SUCCESS! Model loaded!")
except Exception as e:
    print(f"FAILED: {e}")
