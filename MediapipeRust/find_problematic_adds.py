import onnx
from onnx import helper

# Load model
model_path = "models/efficientdet_lite0.onnx"
model = onnx.load(model_path)

# First, let's find all the problematic shapes by looking at Add nodes in fpn_cells
# and see what shapes they have

problem_adds = []

for node in model.graph.node:
    if node.op_type == "Add" and "fpn_cells" in node.name:
        inputs = list(node.input)
        outputs = list(node.output)
        problem_adds.append({
            'name': node.name,
            'inputs': inputs,
            'outputs': outputs
        })

print(f"Found {len(problem_adds)} Add nodes in fpn_cells")

# Let's look at the first problematic one to understand the pattern
if problem_adds:
    first = problem_adds[0]
    print(f"\nFirst problematic Add: {first['name']}")
    print(f"  Inputs: {first['inputs']}")

    # Find shapes of inputs
    for inp in first['inputs']:
        found = False
        for val in model.graph.value_info:
            if val.name == inp:
                if val.type.HasField('tensor_type'):
                    shape = [d.dim_value for d in val.type.tensor_type.shape.dim]
                    print(f"    {inp}: shape = {shape}")
                    found = True
        if not found:
            print(f"    {inp}: NO SHAPE INFO")

        # Check initializers
        for init in model.graph.initializer:
            if init.name == inp:
                print(f"    {inp}: initializer shape = {init.dims}")
