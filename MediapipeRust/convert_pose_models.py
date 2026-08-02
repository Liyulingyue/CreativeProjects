import tf2onnx
import os

models = [
    ("models/pose_extracted/pose_detector.tflite", "models/pose_detector.onnx", ["input_1"], ["Identity", "Identity_1"]),
    ("models/pose_extracted/pose_landmarks_detector.tflite", "models/pose_landmarks_detector.onnx", ["input_1"], ["Identity", "Identity_1", "Identity_2", "Identity_3", "Identity_4"]),
]

for tflite_path, onnx_path, inputs, outputs in models:
    if os.path.exists(onnx_path):
        print(f"Already converted: {onnx_path}")
        continue

    print(f"Converting {tflite_path}...")
    print(f"  Inputs: {inputs}")
    print(f"  Outputs: {outputs}")

    try:
        tf2onnx.convert.from_tflite(
            tflite_path,
            input_names=inputs,
            output_names=outputs,
            output_path=onnx_path
        )
        size = os.path.getsize(onnx_path)
        print(f"  SUCCESS: {onnx_path} ({size} bytes)")
    except Exception as e:
        print(f"  FAILED: {e}")
        import traceback
        traceback.print_exc()

print("\nDone!")
