import onnx
from onnx import helper, numpy_helper
import numpy as np

model_path = "models/efficientdet_lite0.onnx"
model = onnx.load(model_path)

# Find resample_p7 input to Resize__534
print("Checking resample_p7/max_pooling2d_1/MaxPool...")
for node in model.graph.node:
    if "resample_p7/max_pooling2d_1/MaxPool" in node.output:
        print(f"Node: {node.name}")
        print(f"  Inputs: {list(node.input)}")
        for inp in node.input:
            for val in model.graph.value_info:
                if val.name == inp:
                    if val.type.HasField('tensor_type'):
                        shape = val.type.tensor_type.shape.dim
                        print(f"    {inp}: {[(d.dim_value if d.dim_value > 0 else d.dim_param) for d in shape]}")

# Find resample_p6 input to the problematic Add
print("\nChecking resample_p6/max_pooling2d/MaxPool...")
for node in model.graph.node:
    if "resample_p6/max_pooling2d/MaxPool" in node.output:
        print(f"Node: {node.name}")
        print(f"  Inputs: {list(node.input)}")
        for inp in node.input:
            for val in model.graph.value_info:
                if val.name == inp:
                    if val.type.HasField('tensor_type'):
                        shape = val.type.tensor_type.shape.dim
                        print(f"    {inp}: {[(d.dim_value if d.dim_value > 0 else d.dim_param) for d in shape]}")

# Let's manually compute what Resize__534 should output
# scales = [1, 1, 1.67, 1.67]
# input shape = [1, 64, 3, 3]
# output shape should be [1, 64, round(3*1.67), round(3*1.67)] = [1, 64, 5, 5]
print("\n\nExpected Resize output:")
print("  Input: [1, 64, 3, 3]")
print("  Scales: [1, 1, 1.6666666, 1.6666666]")
print("  Expected output: [1, 64, 5, 5] (3 * 1.67 ≈ 5)")

print("\nActual Resize output from value_info:")
for val in model.graph.value_info:
    if val.name == "Resize__534:0":
        if val.type.HasField('tensor_type'):
            shape = val.type.tensor_type.shape.dim
            print(f"  Shape: {[(d.dim_value if d.dim_value > 0 else d.dim_param) for d in shape]}")

print("\nActual MaxPool (resample_p6) output from value_info:")
for val in model.graph.value_info:
    if val.name == "resample_p6/max_pooling2d/MaxPool":
        if val.type.HasField('tensor_type'):
            shape = val.type.tensor_type.shape.dim
            print(f"  Shape: {[(d.dim_value if d.dim_value > 0 else d.dim_param) for d in shape]}")
