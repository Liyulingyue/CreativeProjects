import onnx
from onnx import helper, numpy_helper
import numpy as np

model_path = "models/efficientdet_lite0.onnx"
model = onnx.load(model_path)

# Find the Resize__534 node
for node in model.graph.node:
    if node.name == "Resize__534":
        print(f"Resize node: {node.name}")
        print(f"  Op: {node.op_type}")
        print(f"  Inputs: {list(node.input)}")

        # Find the values of these inputs
        for inp in node.input:
            # Check if it's an initializer
            for init in model.graph.initializer:
                if init.name == inp:
                    arr = numpy_helper.to_array(init)
                    print(f"  Initializer '{inp}': {arr}")

            # Check value_info
            for val in model.graph.value_info:
                if val.name == inp:
                    if val.type.HasField('tensor_type'):
                        shape = val.type.tensor_type.shape.dim
                        print(f"  ValueInfo '{inp}': shape = {[(d.dim_value if d.dim_value > 0 else d.dim_param) for d in shape]}")

# Now let's check what resample_p6/max_pooling2d/MaxPool produces
print("\n\nChecking resample_p6/max_pooling2d/MaxPool output...")
for node in model.graph.node:
    if "resample_p6/max_pooling2d/MaxPool" in node.output:
        print(f"Node: {node.name}")
        print(f"  Op: {node.op_type}")
        for inp in node.input:
            print(f"  Input: {inp}")
            for val in model.graph.value_info:
                if val.name == inp:
                    if val.type.HasField('tensor_type'):
                        shape = val.type.tensor_type.shape.dim
                        print(f"    Shape: {[(d.dim_value if d.dim_value > 0 else d.dim_param) for d in shape]}")

# Let's print the scales__747 initializer value
print("\n\nscales__747 value:")
for init in model.graph.initializer:
    if "scales__747" in init.name:
        arr = numpy_helper.to_array(init)
        print(f"  {init.name}: {arr}")
        print(f"  Shape: {arr.shape}")
