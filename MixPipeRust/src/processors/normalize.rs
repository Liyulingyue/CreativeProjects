use std::any::Any;

use crate::node::{Frame, FrameData, FrameMeta, Node, NodeError, PixelFormat, Result};

pub struct Normalize {
    mean: [f32; 3],
    std: [f32; 3],
}

impl Normalize {
    pub fn new(mean: [f32; 3], std: [f32; 3]) -> Self {
        Self { mean, std }
    }

    pub fn imagenet() -> Self {
        Self {
            mean: [123.675, 116.28, 103.53],
            std: [58.395, 57.12, 57.375],
        }
    }
}

impl Node for Normalize {
    fn process(&self, frame: Frame) -> Result<Frame> {
        let fmt = frame.meta.format;
        let (data, new_meta) = match frame.data {
            FrameData::Rgb(d) => {
                let mut out = Vec::with_capacity(d.len());
                for (i, &v) in d.iter().enumerate() {
                    let ch = i % 3;
                    let normalized = (v as f32 - self.mean[ch]) / self.std[ch];
                    out.push(normalized);
                }
                (
                    FrameData::Rgb(out),
                    FrameMeta {
                        width: frame.meta.width,
                        height: frame.meta.height,
                        timestamp_ms: frame.meta.timestamp_ms,
                        format: PixelFormat::Rgb,
                    },
                )
            }
            FrameData::Rgba(d) => {
                let mut out = Vec::with_capacity(d.len());
                for (i, &v) in d.iter().enumerate() {
                    let ch = i % 4;
                    if ch < 3 {
                        let normalized = (v as f32 - self.mean[ch]) / self.std[ch];
                        out.push(normalized);
                    } else {
                        out.push(v as f32);
                    }
                }
                (
                    FrameData::Rgba(out),
                    FrameMeta {
                        width: frame.meta.width,
                        height: frame.meta.height,
                        timestamp_ms: frame.meta.timestamp_ms,
                        format: PixelFormat::Rgba,
                    },
                )
            }
            _ => return Err(NodeError::UnsupportedFormat(format!("{:?}", fmt))),
        };

        Ok(Frame::new(data, new_meta))
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}
