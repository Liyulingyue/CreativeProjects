import onnx
from onnx import helper, shape_inference

# Load the original TFLite converted ONNX
model_path = "models/efficientdet_lite0.onnx"
model = onnx.load(model_path)

print("Finding the problematic Add node...")

# Find the problematic node
for node in model.graph.node:
    if "Add" in node.op_type and "fpn_cells/cell_0/fnode0/op_after_combine5" in node.name:
        print(f"\nNode: {node.name}")
        print(f"  Op: {node.op_type}")
        print(f"  Inputs: {list(node.input)}")
        print(f"  Outputs: {list(node.output)}")

        # Find the inputs in value_info
        for vi in model.graph.value_info:
            if vi.name == node.input[0] or vi.name == node.input[1]:
                print(f"  Input {vi.name}:")
                if vi.type.HasField('tensor_type'):
                    shape = vi.type.tensor_type.shape.dim
                    print(f"    Shape: [{shape[0].dim_value}, {shape[1].dim_value}, {shape[2].dim_value}, {shape[3].dim_value}]")

print("\n\nAll Add nodes with fpn_cells in name:")
for node in model.graph.node:
    if "Add" in node.op_type and "fpn_cells" in node.name:
        print(f"  {node.name}")
