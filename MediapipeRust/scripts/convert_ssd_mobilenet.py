import tensorflow as tf
import tf2onnx
import os

tflite_path = "models/ssd_mobilenet_v1.tflite"
onnx_path = "models/ssd_mobilenet_v1.onnx"

# Get input/output names
interpreter = tf.lite.Interpreter(model_path=tflite_path)
interpreter.allocate_tensors()

input_detail = interpreter.get_input_details()[0]
output_details = interpreter.get_output_details()

print(f"Input: {input_detail['name']}")
print(f"Outputs:")
for out in output_details:
    print(f"  {out['name']}: {out['shape']}")

# Try to convert
print("\nConverting...")

input_names = [input_detail['name']]
output_names = [out['name'] for out in output_details]

print(f"Input names: {input_names}")
print(f"Output names: {output_names}")

try:
    tf2onnx.convert.from_tflite(
        tflite_path,
        input_names=input_names,
        output_names=output_names,
        output_path=onnx_path
    )
    print(f"Converted successfully!")

    # Test with onnxruntime
    import onnxruntime as ort
    session = ort.InferenceSession(onnx_path)
    print(f"\nONNX Runtime session created successfully!")
    print(f"Inputs: {[(i.name, i.shape) for i in session.get_inputs()]}")
    print(f"Outputs: {[(o.name, o.shape) for o in session.get_outputs()]}")

except Exception as e:
    print(f"Failed: {e}")
    import traceback
    traceback.print_exc()
