# MediaPipe 功能对齐分析

## 概述

本文档记录 MediaPipe 官方功能与当前项目 (mediapipe-rust) 的对齐状态。

**设计目标**：基于 ONNX Runtime 作为主要后端，完善框架逻辑，让框架可用。Rust Native / TF Lite 暂时搁置。

更新时间: 2026-08-03

---

## MediaPipe Tasks

### Vision 任务

| 任务 | 官方 API | 项目状态 | 备注 |
|------|----------|----------|------|
| Image Classification | ✅ | ✅ 已实现 | `image_classification.rs` |
| Object Detection | ✅ | ✅ 已实现 | `object_detection.rs` |
| Face Detection | ✅ | ✅ 已实现 | `face_detection.rs` |
| Face Landmark Detection | ✅ | ✅ 已实现 | `face_landmark.rs` |
| Hand Landmark Detection | ✅ | ✅ 已实现 | `hand_landmark.rs` |
| Pose Landmark Detection | ✅ | ✅ 已实现 | `pose_landmark.rs` |
| Image Segmentation | ✅ | ✅ 已实现 | `image_segmentation.rs` |
| Image Embedding | ✅ | ✅ 已实现 | `image_embedding.rs` |
| Gesture Recognition | ✅ | ✅ 已实现 | `gesture_recognition.rs` |
| Iris Tracking | ✅ | ✅ 已实现 | `iris_tracking.rs` |
| Selfie Segmentation | ✅ | ✅ 已实现 | 使用 deeplab_v3 作为后端 |
| Hair Segmentation | ✅ | ❌ ONNX 模型无效 | 暂无替代方案 |

### Audio 任务

| 任务 | 官方 API | 项目状态 | 备注 |
|------|----------|----------|------|
| Audio Classification | ✅ | ⚠️ 代码完成 | `audio_classification.rs` - 代码完成，缺 ONNX 模型 |
| Audio Embedding | ✅ | ❌ 未实现 | 缺模型 |
| Speech Recognition | ✅ | ❌ 未实现 | 缺模型 |

### Text 任务

| 任务 | 官方 API | 项目状态 | 备注 |
|------|----------|----------|------|
| Text Classification | ✅ | ⚠️ 代码完成 | `text_classification.rs` - 代码完成，缺 ONNX 模型 |
| Text Embedding | ✅ | ⚠️ 代码完成 | `text_embedding.rs` - 代码完成，缺 ONNX 模型 |
| Language Detection | ✅ | ❌ 未实现 | 缺模型 |
| Entity Extraction | ✅ | ❌ 未实现 | 缺模型 |

---

## Audio/Text Tasks 实现规划

### 当前状态

Audio/Text Tasks 的代码框架已完成，但缺少 ONNX 模型。

**实现难度分级**：
- 🟢 简单：已有代码框架，有可用模型或可转换
- 🟡 中等：需要模型转换或较多后处理工作
- 🔴 困难：缺模型，且 MediaPipe 原模型无法转换

### 待办事项

| 优先级 | 任务 | 难度 | 说明 |
|--------|------|------|------|
| P2 | Audio Classification | 🟡 | 需要 YAMNet 或类似 ONNX 模型 |
| P2 | Text Classification | 🟡 | 需要 BERT/文本分类 ONNX |
| P2 | Text Embedding | 🟡 | 需要 BERT/文本向量 ONNX |
| P3 | Audio Embedding | 🔴 | 缺模型 |
| P3 | Speech Recognition | 🔴 | 缺模型 |
| P3 | Language Detection | 🔴 | 缺模型 |
| P3 | Entity Extraction | 🔴 | 缺模型 |

### 建议

1. **近期**：优先保证 Vision Tasks 稳定可用
2. **中期**：按需添加 Audio/Text Tasks
3. **远期**：如果需要完整对齐，再攻克困难的 P3 任务

**注**：MediaPipe 的 Audio/Text Tasks 通常依赖复杂的文本处理（如 BERT tokenizer）或音频处理，这些在纯 Rust 环境可能需要额外工作。

---

## MediaPipe Framework

| 组件 | 状态 | 优先级 |
|------|------|--------|
| Calculator Graph | ❌ 未实现 | P0 |
| Packet-based data flow | ❌ 未实现 | P0 |
| Calculator API | ❌ 未实现 | P0 |
| Graph configuration (.pbtxt) | ❌ 未实现 | P0 |

---

## 后端支持

**当前策略**：ONNX Runtime 作为主要后端，其他后端暂不投入。

| 后端 | 状态 | 优先级 | 备注 |
|------|------|--------|------|
| ONNX Runtime | ⚠️ 主力后端 | **P0** | `onnxruntime.rs` - 需完善 |
| TensorFlow Lite | ⚠️ 暂不投入 | P2 | `tflite.rs` - 功能有限 |
| Native (raw ONNX) | ⚠️ 暂不投入 | P2 | `native.rs` - 缺少优化 |
| MediaPipe C++ | ⚠️ 暂不投入 | P2 | `mediapipe.rs` - 空壳 |

---

## 缺失功能详细说明

### P0 - 核心缺失

1. **Iris Tracking**
   - 用途: 眼球追踪、虹膜检测、深度估计
   - 模型: `iris_landmark.onnx` ✅ 已就绪
   - 输入: `[1, 64, 64, 3]` / 输出: `[(1, 213), (1, 15)]`
   - 状态: ✅ Rust 代码已实现 (`iris_tracking.rs`)

2. **Selfie Segmentation**
   - 用途: 背景分割、人像抠图
   - 问题: ONNX 转换失败，tf2onnx 不支持 TFLite 自定义操作
   - 替代方案: 寻找可用的 ONNX 分割模型

3. **Hair Segmentation**
   - 用途: 发丝分割、虚拟试发色
   - 依赖模型: `hair_segmentation.tflite`
   - 问题: TFLite 使用自定义操作，tf2onnx 不支持
   - 替代方案: 可用 `deeplab_v3` 分割模型替代，或寻找预训练头发分割 ONNX

4. **Calculator Graph 框架**
   - MediaPipe 的核心特性
   - 支持复杂的、多阶段的 ML 流水线
   - 允许自定义 Calculator 扩展
   - 优先级: 远期目标

### P1 - 重要缺失

5. **Audio Embedding**
   - 音频特征提取

6. **Speech Recognition**
   - 语音转文字

7. **Language Detection**
   - 语种识别

8. **Entity Extraction**
   - 命名实体识别

### P2 - 一般缺失

9. **C API 完善**
   - 当前为空壳实现
   - 需要完整实现所有 `mp_*` 函数

---

## 模型转换状态

| 模型 | TFLite 来源 | ONNX 状态 | 备注 |
|------|-------------|-----------|------|
| selfie_segmentation | ✅ | ❌ 失败 | 包含不支持的 Op (`TFL_Convolution2DTransposeBias`) |
| hair_segmentation | ✅ | ❌ 失败 | 包含不支持的 Op (`TFL_MaxPoolingWithArgmax2D` 等) |
| iris_landmark | ✅ | ✅ 成功 | 可用！输入 [1,64,64,3], 输出 [213] + [15] |

**注**: MediaPipe TFLite 模型大量使用自定义操作（`TFL_*` 系列），tf2onnx 转换能力有限。

---

## MediaPipe Task 文件

项目中包含 `.task` 文件（MediaPipe Task 格式，本质是 ZIP），可提取 TFLite 模型：

| Task 文件 | 提取的模型 | 输入 | 输出 |
|-----------|-----------|------|------|
| `hand_landmarker.task` | `hand_detector.tflite`, `hand_landmarks_detector.tflite` | 图片 | 手部关键点 |
| `hand_gesture_recognizer.task` (在 `gesture_extracted/`) | `gesture_embedder.tflite`, `canned_gesture_classifier.tflite` | 128维 embedding | 8 种手势 |
| `audio.task` | (只有标签文件 yamnet_label_list.txt) | - | - |

**注**: 这些 TFLite 模型同样包含自定义操作，tf2onnx 转换可能失败。

---

## 已实现功能清单

### Vision
- [x] Image Classification (`ImageClassifier`)
- [x] Object Detection (`ObjectDetector`)
- [x] Face Detection (`FaceDetector`)
- [x] Face Landmark Detection (`FaceLandmarker`)
- [x] Hand Landmark Detection (`HandLandmarker`)
- [x] Pose Landmark Detection (`PoseLandmarker`)
- [x] Image Segmentation (`ImageSegmenter`)
- [x] Image Embedding (`ImageEmbedder`)
- [x] Gesture Recognition (`GestureRecognizer`)
- [x] Iris Tracking (`IrisTracker`)
- [x] Selfie Segmentation (`SelfieSegmenter`) - 使用 deeplab_v3 实现

### Audio
- [x] Audio Classification (`AudioClassifier`)

### Text
- [x] Text Classification (`TextClassifier`)
- [x] Text Embedding (`TextEmbedder`)

---

## 预处理/后处理

| 功能 | 状态 | 对齐程度 | 备注 |
|------|------|----------|------|
| NonMaxSuppression | ✅ | ✅ 已对齐 | |
| Softmax | ✅ | ✅ 已对齐 | |
| Sigmoid | ✅ | ✅ 已对齐 | |
| Box Decoding | ✅ | ✅ 已对齐 | 支持 SSD 类型 |
| Landmark Normalization | ✅ | ⚠️ 部分 | 未处理 Z 坐标 |
| Clip Boxes | ✅ | ✅ 已对齐 | |
| Image Resize | ⚠️ 基础 | ⚠️ 未对齐 | 使用最近邻，MediaPipe 用 bilinear |
| 归一化参数 | ⚠️ 基础 | ⚠️ 未对齐 | `mean=[127.5,127.5,127.5], std=[127.5,127.5,127.5]` |
| 颜色空间转换 | ⚠️ 基础 | ⚠️ 未对齐 | RGB→BGR 等转换未完整实现 |

**注**: 预处理/后处理的完全对齐需要较大工作量，当前优先保证功能可用。

---

## 后续规划

### Phase 1: 基于 ONNX Runtime 补全 Vision Tasks
**目标**：ORT 后端跑通所有 Vision Tasks

1. 验证已实现的 9 个 Vision Tasks 在 ORT 下正常工作
2. ~~实现 Iris Tracking~~ → ✅ 已实现（iris_tracking.rs）
3. ~~实现 Selfie Segmentation~~ → ✅ 已实现（使用 deeplab_v3 替代）
4. Hair Segmentation 暂无替代方案，标记为永久缺失

### Phase 2: 完善预处理/后处理逻辑
**目标**：输出结果与官方对齐

1. 对齐 MediaPipe 官方预处理（归一化、颜色空间、resize）
2. 对齐后处理（Landmark 坐标、Box 格式、Segmentation Mask）
3. 数值对齐验证（与官方输出对比）

### Phase 3: 补全 Audio/Text Tasks（ORT 后端）
1. Audio Embedding
2. Language Detection
3. Entity Extraction

### Phase 4: 完善 API 对齐
1. API 接口与官方 MediaPipe Tasks 100% 一致
2. Builder 模式、Options 配置对齐

### Phase 5: Framework（远期目标）
1. Calculator Graph 数据结构
2. Packet 系统
3. .pbtxt 配置解析

---

## 与官方 MediaPipe 的差异说明

本文档记录本项目与 Google 官方 MediaPipe 之间的妥协性差异。

### 技术限制导致的差异

| 功能 | 官方实现 | 本项目实现 | 差异说明 |
|------|----------|------------|----------|
| **Selfie Segmentation** | MediaPipe 原生模型 (`selfie_segmentation.tflite`) | deeplab_v3 替代 | MediaPipe 模型包含 `TFL_Convolution2DTransposeBias` 等自定义操作，tf2onnx 无法转换。使用 deeplab_v3 21 类分割模型，取 person 类(15) 作为前景，精度可能略有下降 |
| **Hair Segmentation** | MediaPipe 原生模型 (`hair_segmentation.tflite`) | 未实现 | 同上，tf2onnx 不支持 TFLite 自定义操作，且 deeplab_v3 无法有效分割头发类别 |
| **Iris Tracking** | MediaPipe 原生模型 (`iris_landmark.tflite`) | 待实现 | ONNX 模型已转换成功（iris_landmark.onnx），但后处理逻辑尚未实现 |

### 模型来源与许可证

- 所有 MediaPipe TFLite 模型来自 Google 官方，许可证为 **Apache 2.0**
- 基于 MediaPipe 模型转换的 ONNX 文件同样遵循 Apache 2.0 许可证
- 替代模型 deeplab_v3 同样来自 MediaPipe 官方

### 设计决策

1. **后端选择**: 采用 ONNX Runtime 作为主要推理后端，而非 MediaPipe 原生 C++ 框架
2. **可插拔架构**: 支持多后端，但实际只有 ORT 后端可用
3. **Framework 缺失**: MediaPipe 的 Calculator Graph 框架完全未实现，这是官方架构的核心特色

### 已知限制

1. 部分 MediaPipe TFLite 模型使用自定义操作，tf2onnx 无法转换
2. 预处理/后处理逻辑与官方可能存在细微差异
3. 无法使用 MediaPipe Studio、Model Maker 等官方工具
