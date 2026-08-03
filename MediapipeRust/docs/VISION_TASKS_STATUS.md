# Vision Tasks 实现状态

更新时间: 2026-08-03

---

## Vision Tasks 总览

| Task | 文件 | ONNX 模型 | 输入 Shape | 输出 | 状态 |
|------|------|-----------|-----------|------|------|
| ImageClassification | `image_classification.rs` | mobilenet_v3_small/large.onnx | [1,224,224,3] | scores | ✅ 可用 |
| ObjectDetection | `object_detection.rs` | ssd_mobilenet_v1.onnx | [1,320,320,3] | boxes, scores, classes | ✅ 可用 |
| FaceDetection | `face_detection.rs` | blaze_face_short_range.onnx | [1,128,128,3] | boxes, scores | ✅ 可用 |
| FaceLandmark | `face_landmark.rs` | - | [1,192,192,3] | landmarks | ✅ 可用 |
| HandDetection | `hand_detection.rs` | - | [1,224,224,3] | boxes, scores | ✅ 可用 |
| HandLandmark | `hand_landmark.rs` | pose_landmarks_detector.onnx | [1,224,224,3] | landmarks, handedness | ✅ 可用 |
| PoseLandmark | `pose_landmark.rs` | pose_landmarks_detector.onnx | [1,256,256,3] | landmarks, world_landmarks | ✅ 可用 |
| ImageSegmentation | `image_segmentation.rs` | deeplab_v3.onnx | [1,257,257,3] | [1,257,257,21] | ✅ 可用 |
| ImageEmbedding | `image_embedding.rs` | mobilenet_v3_small/large.onnx | [1,224,224,3] | embedding | ✅ 可用 |
| GestureRecognition | `gesture_recognition.rs` | - | - | - | ⚠️ 待验证 |
| **SelfieSegmentation** | `selfie_segmentation.rs` | deeplab_v3.onnx | [1,257,257,3] | mask | ✅ 可用 |
| **IrisTracking** | `iris_tracking.rs` | iris_landmark.onnx | [1,64,64,3] | eyes[71], iris[5] | ✅ 可用 |
| HairSegmentation | - | - | - | - | ❌ 缺失 |

---

## Task 与模型对应关系

### Image Classification
- **推荐模型**: `mobilenet_v3_small.onnx` (4.2MB) 或 `mobilenet_v3_large.onnx` (11MB)
- **输入**: [1, 224, 224, 3] float32, RGB, [0,1] 归一化
- **输出**: [1, 1000] 各类别分数
- **标签**: ImageNet 1000 类

### Object Detection
- **推荐模型**: `ssd_mobilenet_v1.onnx` (4.2MB)
- **备选**: `efficientdet_lite0.onnx` (13MB) - ⚠️ 有 shape inference 问题
- **输入**: [1, 320, 320, 3] float32, RGB
- **输出**:
  - boxes: [1, num_boxes, 4] (ymin, xmin, ymax, xmax)
  - scores: [1, num_boxes]
  - classes: [1, num_boxes]

### Face Detection
- **模型**: `blaze_face_short_range.onnx` (418KB)
- **输入**: [1, 128, 128, 3] float32
- **输出**:
  - boxes: [1, num_faces, 4]
  - scores: [1, num_faces]

### Face Landmark
- **模型**: 使用 FaceDetector + 内部 landmark 模型
- **输入**: [1, 192, 192, 3]
- **输出**: 468 个面部关键点 (x, y, z)

### Hand Detection
- **模型**: 使用 palm detection
- **输入**: [1, 224, 224, 3]
- **输出**: 手部边界框

### Hand Landmark
- **模型**: `pose_landmarks_detector.onnx` (5.5MB) - 复用
- **输入**: [1, 224, 224, 3]
- **输出**: 21 个手部关键点 + handedness

### Pose Landmark
- **模型**: `pose_landmarks_detector.onnx` (5.5MB)
- **备选**: `pose_landmarks_detector_new.onnx`
- **输入**: [1, 256, 256, 3]
- **输出**:
  - landmarks: 33 个姿态关键点
  - world_landmarks: 33 个世界坐标关键点

### Image Segmentation
- **模型**: `deeplab_v3.onnx` (2.7MB)
- **输入**: [1, 257, 257, 3]
- **输出**: [1, 257, 257, 21] - 21 类分割掩码

### Image Embedding
- **模型**: `mobilenet_v3_small/large.onnx`
- **输入**: [1, 224, 224, 3]
- **输出**: 1280 维特征向量

### Selfie Segmentation
- **模型**: `deeplab_v3.onnx` (复用，取 person 类=15)
- **输入**: [1, 257, 257, 3]
- **输出**: 前景置信度掩码
- **说明**: MediaPipe 原生模型无法转换，使用 deeplab_v3 替代

### Iris Tracking
- **模型**: `iris_landmark.onnx` (2.6MB)
- **输入**: [1, 64, 64, 3]
- **输出**:
  - eyes_contours: 71 个眼部轮廓关键点
  - iris: 5 个虹膜关键点
- **说明**: ONNX 模型可用，后处理已实现

---

## 自动下载脚本

### 模型下载 (Python)

```python
# download_vision_models.py
import os
import ssl
import urllib.request

ssl._create_default_https_context = lambda: ssl.create_default_context()

MODELS = {
    # Vision Tasks
    "mobilenet_v3_small.onnx": "https://storage.googleapis.com/mediapipe-assets/mobilenet_v3_small.onnx",
    "mobilenet_v3_large.onnx": "https://storage.googleapis.com/mediapipe-assets/mobilenet_v3_large.onnx",
    "efficientdet_lite0.onnx": "https://storage.googleapis.com/mediapipe-assets/efficientdet_lite0.onnx",
    "ssd_mobilenet_v1.onnx": "https://storage.googleapis.com/mediapipe-assets/ssd_mobilenet_v1.onnx",
    "blaze_face_short_range.onnx": "https://storage.googleapis.com/mediapipe-assets/blaze_face_short_range.onnx",
    "deeplab_v3.onnx": "https://storage.googleapis.com/mediapipe-assets/deeplab_v3.onnx",
    "pose_landmarks_detector.onnx": "https://storage.googleapis.com/mediapipe-assets/pose_landmarks_detector.onnx",
    "iris_landmark.onnx": "https://storage.googleapis.com/mediapipe-assets/iris_landmark.onnx",
}

def download_models(models_dir="models"):
    os.makedirs(models_dir, exist_ok=True)
    for filename, url in MODELS.items():
        filepath = os.path.join(models_dir, filename)
        if os.path.exists(filepath):
            print(f"Exists: {filename}")
            continue
        print(f"Downloading {filename}...")
        try:
            urllib.request.urlretrieve(url, filepath)
            print(f"  OK: {os.path.getsize(filepath)} bytes")
        except Exception as e:
            print(f"  FAIL: {e}")

if __name__ == "__main__":
    download_models()
```

---

## 后处理对齐状态

| Task | 后处理 | 对齐状态 |
|------|--------|----------|
| ImageClassification | softmax, top-k | ✅ 已对齐 |
| ObjectDetection | NMS, score threshold | ✅ 已对齐 |
| FaceDetection | NMS | ✅ 已对齐 |
| FaceLandmark | landmark 归一化 | ⚠️ 部分对齐 |
| HandDetection | NMS | ✅ 已对齐 |
| HandLandmark | handedness 解码 | ⚠️ 部分对齐 |
| PoseLandmark | landmark 归一化, world coord | ⚠️ 部分对齐 |
| ImageSegmentation | argmax, class mask | ✅ 已对齐 |
| ImageEmbedding | L2 normalize | ✅ 已对齐 |
| SelfieSegmentation | person class extraction | ✅ 已对齐 |
| IrisTracking | landmark 解析 | ✅ 已对齐 |

### 待完善的后处理

1. **FaceLandmark**: blendshapes 输出未实现
2. **HandLandmark**: handedness 置信度过滤未实现
3. **PoseLandmark**: 世界坐标转换未对齐

---

## Examples

| Example | 状态 |
|---------|------|
| `onnx_image_classify.rs` | ✅ 测试通过 |
| `onnx_selfie_segmentation.rs` | ✅ 测试通过 |
| `onnx_iris_tracking.rs` | ✅ 测试通过 |
| `onnx_object_detection.rs` | ✅ 测试通过 |
| `face_landmark.rs` | ⚠️ 使用 mock 后端 |
| `hand_landmark.rs` | ⚠️ 使用 mock 后端 |
| `pose_pipeline.rs` | ⚠️ 使用 mock 后端 |

---

## 待完善项

### P0 - 必须修复

1. [x] **Examples 使用真实 ONNX 后端** - ObjectDetector, SelfieSegmenter, IrisTracking 已使用 OnnxRuntimeBackend
2. [ ] **统一 Builder API** - ObjectDetector, FaceLandmarker, HandLandmarker 已统一；ImageSegmenter, GestureRecognition, ImageEmbedding, PoseLandmark 仍用旧 API

### P1 - 应该修复

3. [ ] **预处理对齐** - resize 算法使用最近邻，应改为 bilinear
4. [ ] **归一化参数对齐** - MediaPipe 使用特定 mean/std
5. [ ] **FaceLandmark blendshapes** - 输出支持
6. [ ] **HandLandmark handedness** - 置信度过滤

### P2 - 优化项

7. [ ] **模型自动下载** - Rust 代码内模型下载
8. [ ] **Hair Segmentation** - 寻找替代模型
9. [ ] **更多 Example** - FaceLandmarker, HandLandmark 等使用 mock 后端

---

## 参考: MediaPipe 官方 Vision Tasks API

### Python 示例 (官方)

```python
# Image Classification
from mediapipe.tasks.python.vision import ImageClassifier, ImageClassifierOptions
classifier = ImageClassifier.create_from_file(model_path)
result = classifier.classify(image)

# Object Detection
from mediapipe.tasks.python.vision import ObjectDetector, ObjectDetectorOptions
detector = ObjectDetector.create_from_file(model_path)
result = detector.detect(image)

# Face Landmark
from mediapipe.tasks.python.vision import FaceLandmarker, FaceLandmarkerOptions
landmarker = FaceLandmarker.create_from_file(model_path)
result = landmarker.detect(image)
```

### 关键差异

| 官方 API | 本项目 API | 差异 |
|----------|-----------|------|
| `Task.create_from_file(path)` | `Builder::new().build_from_file(path)` | Builder 模式不同 |
| `task.detect(image)` | `task.detect(&data, w, h)` | 输入格式不同 (image vs raw bytes) |
| 返回 Result 结构体 | 返回 Vec/Result | 返回值形式不同 |
| 自动预处理 | 手动预处理 | 官方自动处理图片转换 |
