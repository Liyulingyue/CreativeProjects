import onnx
from onnx import helper, numpy_helper

model_path = "models/efficientdet_lite0_fixed.onnx"

print(f"Loading model: {model_path}")
model = onnx.load(model_path)

# Find the problematic node
problematic_nodes = []
for node in model.graph.node:
    if "fpn_cells/cell_0/fnode0/op_after_combine5/Relu6" in node.name:
        problematic_nodes.append(node)
        print(f"\nFound problematic node: {node.name}")
        print(f"  Op: {node.op_type}")
        print(f"  Inputs: {list(node.input)}")
        print(f"  Outputs: {list(node.output)}")

# Try to trace the issue
print("\n\nSearching for shape inconsistencies...")

# Get all tensors and their shapes from value_info
shape_info = {}
for vi in model.graph.value_info:
    shape_info[vi.name] = vi

# Check inputs to the problematic node
if problematic_nodes:
    node = problematic_nodes[0]
    print(f"\nNode inputs:")
    for inp in node.input:
        if inp in shape_info:
            print(f"  {inp}: {shape_info[inp]}")
        else:
            print(f"  {inp}: no shape info")

# Let's also check if there's a Resize or Upsample node nearby that might cause the issue
print("\n\nAll nodes with 'fpn_cells' in name:")
for node in model.graph.node:
    if "fpn_cells" in node.name:
        print(f"  {node.name} ({node.op_type})")
