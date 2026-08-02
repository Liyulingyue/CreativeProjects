import onnx
from onnx import helper, numpy_helper

# Load the converted ONNX model
model_path = "models/efficientdet_lite0.onnx"
model = onnx.load(model_path)

print("Analyzing the problematic Add node in detail...")

# Find the specific problematic node
problematic_name = "fpn_cells/cell_0/fnode0/op_after_combine5/Relu6;fpn_cells/cell_0/fnode0/add_n/add"

# Find the node
for node in model.graph.node:
    if node.name == problematic_name:
        print(f"\nNode: {node.name}")
        print(f"  Op: {node.op_type}")
        print(f"  Inputs: {list(node.input)}")
        print(f"  Outputs: {list(node.output)}")

        # Find the input values in the graph
        for inp_name in node.input:
            for val in model.graph.value_info:
                if val.name == inp_name:
                    print(f"\n  Input '{inp_name}':")
                    if val.type.HasField('tensor_type'):
                        shape = val.type.tensor_type.shape.dim
                        print(f"    Shape: {[(d.dim_value if d.dim_value > 0 else d.dim_param) for d in shape]}")

# Let's also check the initializers and constant tensors
print("\n\nSearching for Resize__534:0 in initializers...")
for init in model.graph.initializer:
    if "Resize__534" in init.name:
        print(f"Found: {init.name}")
        print(f"  Dims: {init.dims}")

# Find the actual Resize node
print("\n\nSearching for Resize node...")
for node in model.graph.node:
    if "Resize__534" in node.name or (node.op_type == "Resize" and any("534" in inp for inp in node.input)):
        print(f"Resize node: {node.name}")
        print(f"  Op: {node.op_type}")
        print(f"  Inputs: {list(node.input)}")
        print(f"  Outputs: {list(node.output)}")

# Try to find what's producing Resize__534:0
print("\n\nFinding producer of Resize__534:0...")
for node in model.graph.node:
    if "Resize__534" in node.output[0]:
        print(f"Producer: {node.name}")
        print(f"  Op: {node.op_type}")
        print(f"  Inputs: {list(node.input)}")
        print(f"  Outputs: {list(node.output)}")
