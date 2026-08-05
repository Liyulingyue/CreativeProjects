# MixPipeRust

受 MediaPipe 启发，用 Rust 编写的可组合多模态推理流水线框架。

## 特性

- **简洁 API**：仅需几行代码即可加载模型并运行推理
- **自动下载**：从 ModelScope 自动下载预训练模型（按需下载，只下载你需要的）
- **流水线支持**：串联检测 + 姿态模型，实现端到端推理
- **可视化支持**：内置 Visualizer，支持绘制边界框、关键点、骨架
- **ONNX Runtime**：通过 ONNX Runtime 实现高性能推理

## 支持的模型

| 模型 | 描述 | 输出 |
|------|------|------|
| RTMDet-Tiny | 目标检测（仅支持 person） | 边界框 |
| RTMPose-Body | 人体关键点检测 | 17 关键点 |
| RTMPose-Face | 人脸关键点检测 | 68 关键点 |
| RTMPose-Hand | 手部关键点检测 | 21 关键点 |
| RTMPose-WholeBody | 全身关键点检测 | 133 关键点 |

## 快速开始

```rust
use mixpiperust::{RtmDet, RtmPose, PretrainedModel, Pipeline};

// 方式一：自动下载（推荐，首次使用时自动从 ModelScope 下载）
let model = RtmDet::from_pretrained(PretrainedModel::RtmDetTiny)?;
// 方式二：使用本地模型
// let model = RtmDet::from_file("path/to/your/rtmdet.onnx")?;

let detections = model.infer(&pixels, width, height)?;

// 姿态估计流水线
let pipeline = Pipeline::builder()
    .detector_model(PretrainedModel::RtmDetTiny)
    .pose_model(PretrainedModel::RtmPoseWholeBody)
    .build()?;

// 或使用本地模型：
// let pipeline = Pipeline::builder()
//     .detector("path/to/rtmdet.onnx")
//     .pose("path/to/rtmpose.onnx")
//     .build()?;

let persons = pipeline.run(&pixels, width, height)?;
for person in persons {
    println!("bbox={:?}", person.bbox);
    for kp in person.keypoints {
        println!("  ({:.1}, {:.1}) conf={:.3}", kp.x, kp.y, kp.confidence);
    }
}
```

## 文档

详细的 API 文档请参考 [docs/API.md](docs/API.md)。

## 安装

```toml
[dependencies]
mixpiperust = "0.1"
image = "0.25"  # 用于图片加载
```

## 示例

```bash
# 检测示例
cargo run --example detection

# 姿态估计示例
cargo run --example pose_estimation

# Pipeline 完整流程示例
cargo run --example pipeline

# 可视化示例
cargo run --example visualization
```

## 可视化

```rust
use mixpiperust::{Pipeline, PretrainedModel, Visualizer};

let pipeline = Pipeline::builder()
    .detector_model(PretrainedModel::RtmDetTiny)
    .pose_model(PretrainedModel::RtmPoseBody)
    .build()?;

let persons = pipeline.run(&pixels, w, h)?;

let mut img = image::open("input.jpg")?.to_rgb8();
let viz = Visualizer::coco17();  // 或 wholebody133(), face68(), hand21()

for person in &persons {
    viz.draw_person(&mut img, person);
}

img.save("output.png")?;
```

## 许可证

MIT
