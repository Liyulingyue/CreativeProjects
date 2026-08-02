import os
import tensorflow as tf
import tf2onnx
import ssl

ssl._create_default_https_context = lambda: ssl.create_default_context()

models_to_convert = [
    ("deeplab_v3.tflite", "deeplab_v3.onnx", ["sub_7"], ["ResizeBilinear_3"]),
    ("blaze_face_short_range.tflite", "blaze_face_short_range.onnx", ["input"], ["regressors", "classificators"]),
]

models_dir = "models"

for tflite_file, onnx_file, input_names, output_names in models_to_convert:
    tflite_path = os.path.join(models_dir, tflite_file)
    onnx_path = os.path.join(models_dir, onnx_file)

    if os.path.exists(onnx_path):
        print(f"Already converted: {onnx_file}")
        continue

    print(f"Converting {tflite_file} -> {onnx_file}...")
    print(f"  Inputs: {input_names}")
    print(f"  Outputs: {output_names}")

    try:
        tf2onnx.convert.from_tflite(
            tflite_path,
            input_names=input_names,
            output_names=output_names,
            output_path=onnx_path
        )

        if os.path.exists(onnx_path):
            size = os.path.getsize(onnx_path)
            print(f"  SUCCESS: {onnx_file} ({size} bytes)")
        else:
            print(f"  Failed: output file not created")

    except Exception as e:
        print(f"  Failed: {e}")
        import traceback
        traceback.print_exc()

print("\nAll conversions complete!")
