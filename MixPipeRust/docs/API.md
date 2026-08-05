# MixPipeRust API 文档

## 目录

- [快速开始](#快速开始)
- [核心概念](#核心概念)
- [模型管理 (model_hub)](#模型管理-model_hub)
- [节点 (Node)](#节点-node)
- [检测模型 (RtmDet)](#检测模型-rtmdet)
- [姿态模型 (RtmPose)](#姿态模型-rtmpose)
- [流水线 (Pipeline)](#流水线-pipeline)
- [数据类型](#数据类型)
- [图像处理 (Processors)](#图像处理-processors)
- [示例代码](#示例代码)

---

## 快速开始

### 方式一：使用预训练模型（推荐）

```rust
use mixpiperust::{RtmDet, RtmPose, PretrainedModel, Frame};

let detector = RtmDet::from_pretrained(PretrainedModel::RtmDetTiny)?;
let pose = RtmPose::from_pretrained(PretrainedModel::RtmPoseWholeBody)?;

// 输入 RGB 图像
let frame = Frame::from_rgb(pixels, width, height);
let frame = detector.process(frame)?;
// frame.meta.custom["detections"] 包含检测结果
```

### 方式二：使用 Pipeline（检测+姿态一体化）

```rust
use mixpiperust::{Pipeline, PretrainedModel};

let pipeline = Pipeline::builder()
    .detector(PretrainedModel::RtmDetTiny)
    .pose(PretrainedModel::RtmPoseWholeBody)
    .build()?;

let persons = pipeline.run(&pixels, width, height)?;
for person in persons {
    println!("bbox: {:?}", person.bbox);
    println!("keypoints: {:?}", person.keypoints);
}
```

---

## 核心概念

### Frame

`Frame` 是数据流的基本单位，包含图像/音频/文本数据和元数据。

```rust
pub struct Frame {
    pub data: FrameData,      // 实际数据
    pub meta: FrameMeta,       // 元数据
}
```

### Node

`Node` 是处理单元的抽象 trait，任何模型或处理器都实现它：

```rust
pub trait Node: Send + Sync {
    fn process(&self, frame: Frame) -> Result<Frame>;
    fn as_any(&self) -> &dyn Any;
}
```

### Pipeline

`Pipeline` 将多个 Node 串联起来，形成完整的推理流水线（如 检测→crop→姿态）。

---

## 模型管理 (model_hub)

### PretrainedModel 预训练模型枚举

```rust
pub enum PretrainedModel {
    RtmDetTiny,          // RTMDet 目标检测（仅支持 person class=0）
    RtmPoseBody,         // 人体 17 关键点
    RtmPoseFace,         // 人脸 68 关键点
    RtmPoseHand,         // 手部 21 关键点
    RtmPoseWholeBody,    // 全身 133 关键点
}
```

### 获取模型路径

```rust
// 获取缓存目录
pub fn get_cache_dir() -> Option<PathBuf>
// 返回: Windows: %LOCALAPPDATA%\mixpiperust\models

// 获取特定模型的缓存路径
pub fn get_model_path(model: PretrainedModel) -> Option<PathBuf>
```

### 下载模型

```rust
// 异步下载
pub async fn download_model(model: PretrainedModel) -> anyhow::Result<PathBuf>

// 同步下载（阻塞）
pub fn download_model_blocking(model: PretrainedModel) -> anyhow::Result<PathBuf>
```

**示例：**

```rust
use mixpiperust::{download_model_blocking, get_model_path, PretrainedModel};

// 检查模型是否已缓存
if let Some(path) = get_model_path(PretrainedModel::RtmDetTiny) {
    if path.exists() {
        println!("模型已缓存: {:?}", path);
    } else {
        // 下载
        let path = download_model_blocking(PretrainedModel::RtmDetTiny)?;
    }
}
```

---

## 节点 (Node)

### Frame 结构

```rust
// 帧数据
pub enum FrameData {
    Image(ImageData),   // 图像数据
    Audio(AudioData),   // 音频数据
    Text(TextData),     // 文本数据
    Video(VideoData),   // 视频数据
    Unknown,
}

// 图像数据结构
pub struct ImageData {
    pub width: u32,           // 宽度
    pub height: u32,          // 高度
    pub format: PixelFormat,  // 像素格式
    pub pixels: Vec<u8>,      // 像素数据
}

// 帧元数据
pub struct FrameMeta {
    pub timestamp_ms: u64,     // 时间戳（毫秒）
    pub source: String,        // 数据来源标识
    pub media_type: MediaType, // 媒体类型
    pub custom: HashMap<String, serde_json::Value>, // 自定义数据
}

// 像素格式
pub enum PixelFormat {
    Rgb,
    Rgba,
    Bgr,
    Bgra,
    Gray,
    Yuv420,
    Unknown,
}

// 媒体类型
pub enum MediaType {
    Image,
    Audio,
    Text,
    Video,
    Unknown,
}
```

### Frame 辅助方法

```rust
impl Frame {
    // 从 RGB 数据创建 Frame
    pub fn from_rgb(pixels: Vec<u8>, width: u32, height: u32) -> Self

    // 获取检测结果
    pub fn detections(&self) -> Option<Vec<Detection>>

    // 获取关键点
    pub fn keypoints(&self) -> Option<Vec<Keypoint>>

    // 设置检测结果
    pub fn set_detections(&mut self, detections: Vec<Detection>)

    // 设置关键点
    pub fn set_keypoints(&mut self, keypoints: Vec<Keypoint>)
}
```

### 数据类型

```rust
// 检测结果
pub struct Detection {
    pub bbox: [f32; 4],   // [x1, y1, x2, y2] 边界框
    pub score: f32,        // 置信度
    pub label: i32,        // 类别标签 (0=person)
}

// 关键点
pub struct Keypoint {
    pub x: f32,            // X 坐标
    pub y: f32,           // Y 坐标
    pub confidence: f32,  // 置信度
}

// 人物（检测+关键点）
pub struct Person {
    pub bbox: [f32; 4],           // 边界框
    pub keypoints: Vec<Keypoint>, // 关键点列表
}
```

### 错误类型

```rust
pub enum NodeError {
    Process(String),              // 处理错误
    UnsupportedFormat(String),   // 不支持的格式
    UnsupportedMediaType(MediaType), // 不支持的媒体类型
    Model(String),               // 模型错误
    Source(String),              // 源错误
}

pub type Result<T> = std::result::Result<T, NodeError>;
```

### crop_frame 裁剪函数

```rust
pub fn crop_frame(frame: &Frame, bbox: &[f32; 4]) -> Result<Frame>
```

按指定边界框裁剪图像帧。

---

## 检测模型 (RtmDet)

### 创建实例

```rust
use mixpiperust::{RtmDet, PretrainedModel};

impl RtmDet {
    // 从本地 ONNX 文件加载
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self>

    // 从预训练模型加载（自动下载）
    pub fn from_pretrained(model: PretrainedModel) -> Result<Self>
}
```

### 推理

```rust
pub fn infer(&self, pixels: &[u8], width: u32, height: u32) -> Result<Vec<Detection>>
```

- **输入**: RGB 格式的像素数据
- **输出**: 检测到的目标列表（已过滤只保留 person, label=0, score>0.3）
- **NMS**: IoU threshold = 0.65

### Node 实现

```rust
impl Node for RtmDet {
    fn process(&self, frame: Frame) -> Result<Frame>
}
```

将检测结果存入 `frame.meta.custom["detections"]`。

### 示例

```rust
use mixpiperust::{RtmDet, PretrainedModel, Frame};

let detector = RtmDet::from_pretrained(PretrainedModel::RtmDetTiny)?;
let frame = Frame::from_rgb(pixels, width, height);
let frame = detector.process(frame)?;

if let Some(dets) = frame.detections() {
    for det in dets {
        println!("Person: bbox={:?}, score={}", det.bbox, det.score);
    }
}
```

---

## 姿态模型 (RtmPose)

### 创建实例

```rust
use mixpiperust::{RtmPose, PretrainedModel};

impl RtmPose {
    // 从本地 ONNX 文件加载
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self>

    // 从预训练模型加载（自动下载）
    pub fn from_pretrained(model: PretrainedModel) -> Result<Self>
}
```

**输入尺寸**：
- Body / WholeBody: 256×192
- Face / Hand: 256×256

### 推理

```rust
pub fn infer(&self, pixels: &[u8], width: u32, height: u32) -> Result<Vec<Keypoint>>
```

- **输入**: RGB 格式的裁剪人物图像
- **输出**: 关键点列表

**关键点数量**：
| 模型 | 数量 |
|------|------|
| Body | 17 |
| Face | 68 |
| Hand | 21 |
| WholeBody | 133 |

### Node 实现

```rust
impl Node for RtmPose {
    fn process(&self, frame: Frame) -> Result<Frame>
}
```

将关键点存入 `frame.meta.custom["keypoints"]`。

### 示例

```rust
use mixpiperust::{RtmPose, PretrainedModel, Frame};

let pose = RtmPose::from_pretrained(PretrainedModel::RtmPoseWholeBody)?;

// 输入应该是裁剪后的人物区域
let frame = Frame::from_rgb(cropped_pixels, crop_width, crop_height);
let frame = pose.process(frame)?;

if let Some(kpts) = frame.keypoints() {
    for (i, kp) in kpts.iter().enumerate() {
        println!("Point {}: ({}, {}) conf={}", i, kp.x, kp.y, kp.confidence);
    }
}
```

---

## 流水线 (Pipeline)

`Pipeline` 将 RTMDet 检测器与 RTMPose 姿态估计器串联，自动完成：
1. 检测图片中的所有人
2. 裁剪每个人体区域
3. 姿态估计
4. 将关键点坐标映射回原图

### 创建 Pipeline

```rust
pub struct Pipeline { /* 私有 */ }

impl Pipeline {
    // 使用 Builder 模式创建
    pub fn builder() -> PipelineBuilder

    // 直接从文件创建
    pub fn from_files(detector_path: &Path, pose_path: &Path) -> Result<Self>
}

pub struct PipelineBuilder { /* 私有 */ }

impl PipelineBuilder {
    pub fn new() -> Self

    // 设置检测模型（支持本地路径或 PretrainedModel）
    pub fn detector<P: AsRef<Path>>(self, path: P) -> Self

    // 设置姿态模型（支持本地路径或 PretrainedModel）
    pub fn pose<P: AsRef<Path>>(self, path: P) -> Self

    // 以下是 pose 的别名：
    pub fn body_pose<P: AsRef<Path>>(self, path: P) -> Self
    pub fn wholebody_pose<P: AsRef<Path>>(self, path: P) -> Self
    pub fn face_pose<P: AsRef<Path>>(self, path: P) -> Self
    pub fn hand_pose<P: AsRef<Path>>(self, path: P) -> Self

    pub fn build(self) -> Result<Pipeline>
}
```

### 运行推理

```rust
pub fn run(&self, pixels: &[u8], width: u32, height: u32) -> Result<Vec<Person>>
```

- **输入**: 原图 RGB 像素数据
- **输出**: 检测到的所有人及其关键点

### 示例

```rust
use mixpiperust::{Pipeline, PretrainedModel};

let pipeline = Pipeline::builder()
    .detector(PretrainedModel::RtmDetTiny)
    .pose(PretrainedModel::RtmPoseWholeBody)
    .build()?;

let persons = pipeline.run(&image_pixels, width, height)?;

for person in &persons {
    println!("检测到 Person: bbox={:?}", person.bbox);
    println!("关键点数量: {}", person.keypoints.len());
}
```

---

## 图像处理 (Processors)

处理器模块 `mixpiperust::processors` 包含图像预处理节点。

### Resize 调整尺寸

```rust
use mixpiperust::processors::Resize;
use crate::node::Node;

let resize = Resize::new(640, 480);
let resized_frame = resize.process(frame)?;
```

### Normalize 归一化

```rust
use mixpiperust::processors::Normalize;

let normalize = Normalize::new([123.675, 116.28, 103.53], [58.395, 57.12, 57.375]);
// 或使用 ImageNet 标准
let normalize = Normalize::imagenet();
```

### ColorConvert 颜色转换

```rust
use mixpiperust::processors::ColorConvert;
use mixpiperust::node::PixelFormat;

let converter = ColorConvert::new(PixelFormat::Rgba);
let rgba_frame = converter.process(rgb_frame)?;
```

---

## 示例代码

### 示例 1: 基础检测

`examples/detection.rs`

```rust
use mixpiperust::{RtmDet, PretrainedModel, Frame};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let detector = RtmDet::from_pretrained(PretrainedModel::RtmDetTiny)?;

    // 加载图片
    let img = image::open("input.jpg")?.to_rgb8();
    let (w, h) = img.dimensions();
    let pixels = img.into_raw();

    let frame = Frame::from_rgb(pixels, w, h);
    let frame = detector.process(frame)?;

    if let Some(dets) = frame.detections() {
        println!("检测到 {} 个人", dets.len());
        for det in dets {
            println!("  bbox: {:?}", det.bbox);
        }
    }

    Ok(())
}
```

### 示例 2: 姿态估计

`examples/pose_estimation.rs`

```rust
use mixpiperust::{RtmPose, PretrainedModel, Frame};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let pose = RtmPose::from_pretrained(PretrainedModel::RtmPoseWholeBody)?;

    // 加载并裁剪图片（需要先检测）
    let img = image::open("input.jpg")?.to_rgb8();
    let pixels = img.into_raw();

    // 假设已知裁剪区域
    let frame = Frame::from_rgb(pixels, 256, 256);
    let frame = pose.process(frame)?;

    if let Some(kpts) = frame.keypoints() {
        println!("检测到 {} 个关键点", kpts.len());
    }

    Ok(())
}
```

### 示例 3: Pipeline 完整流程

`examples/pipeline.rs`

```rust
use mixpiperust::{Pipeline, PretrainedModel};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let pipeline = Pipeline::builder()
        .detector(PretrainedModel::RtmDetTiny)
        .pose(PretrainedModel::RtmPoseWholeBody)
        .build()?;

    let img = image::open("input.jpg")?.to_rgb8();
    let (w, h) = img.dimensions();
    let pixels = img.into_raw();

    let persons = pipeline.run(&pixels, w, h)?;

    println!("检测到 {} 个人", persons.len());
    for (i, person) in persons.iter().enumerate() {
        println!("Person {}: {} 个关键点", i, person.keypoints.len());
    }

    Ok(())
}
```

---

## 可视化 (Visualizer)

`Visualizer` 模块提供绘制姿态估计结果的功能。

### 创建 Visualizer

```rust
use mixpiperust::Visualizer;

// COCO 17 点（人体）
let viz = Visualizer::coco17();

// 全身 133 点
let viz = Visualizer::wholebody133();

// 人脸 68 点
let viz = Visualizer::face68();

// 手部 21 点
let viz = Visualizer::hand21();
```

### 绘制方法

```rust
// 绘制单个人（边界框 + 关键点 + 骨架）
pub fn draw_person(&self, image: &mut RgbImage, person: &Person)

// 只绘制边界框
pub fn draw_bbox(&self, image: &mut RgbImage, bbox: &[f32; 4])

// 只绘制关键点
pub fn draw_keypoints(&self, image: &mut RgbImage, keypoints: &[Keypoint], radius: u32)

// 只绘制骨架连线
pub fn draw_skeleton(&self, image: &mut RgbImage, keypoints: &[Keypoint])
```

### 可视化示例

```rust
use mixpiperust::{Pipeline, PretrainedModel, Visualizer};
use image::RgbImage;

let pipeline = Pipeline::builder()
    .detector_model(PretrainedModel::RtmDetTiny)
    .pose_model(PretrainedModel::RtmPoseBody)
    .build()?;

let img = image::open("input.jpg")?.to_rgb8();
let (w, h) = img.dimensions();
let pixels = img.as_raw().to_vec();

let persons = pipeline.run(&pixels, w, h)?;

let mut output = img;
let viz = Visualizer::coco17();

for person in &persons {
    viz.draw_person(&mut output, person);
}

output.save("output.png")?;
```

### 骨架连接定义

| 模型 | 关键点数 | 骨架定义 |
|------|---------|---------|
| Body (COCO) | 17 | 鼻子、眼睛、耳朵、肩膀、手肘、手腕、髋关节、膝盖、脚踝 |
| WholeBody | 133 | 包含 Body + 面部 + 手部 + 脚部更多细节 |
| Face | 68 | 面部轮廓和特征点 |
| Hand | 21 | 手部手指关节点 |

### 颜色方案

- **Body**: 头部红色、眼睛黄色、上肢绿色/青色、下肢紫色/粉色
- **Skeleton**: 白色连线

---

## 导入参考

```rust
// 完整导入
use mixpiperust::{
    // 核心类型
    Node, Frame, FrameMeta, FrameData, ImageData, AudioData, TextData, VideoData,
    MediaType, PixelFormat, NodeError, Result,
    // 数据结构
    Detection, Keypoint, Person,
    // 模型
    RtmDet, RtmPose, PretrainedModel,
    // 流水线
    Pipeline, PipelineBuilder, PoseModel,
    // 可视化
    Visualizer,
    // 预处理器
    processors::{Resize, Normalize, ColorConvert},
    // 工具函数
    download_model, download_model_blocking, get_model_path, get_cache_dir,
};

// prelude（常用导入）
use mixpiperust::prelude::*;
```
