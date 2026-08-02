import tensorflow as tf

models = [
    'models/pose_extracted/pose_detector.tflite',
    'models/pose_extracted/pose_landmarks_detector.tflite'
]

for model_path in models:
    print(f'\n=== {model_path} ===')
    interpreter = tf.lite.Interpreter(model_path=model_path)
    interpreter.allocate_tensors()

    print('Inputs:')
    for detail in interpreter.get_input_details():
        print(f"  {detail['name']}: shape={detail['shape']}")

    print('Outputs:')
    for detail in interpreter.get_output_details():
        print(f"  {detail['name']}: shape={detail['shape']}")
