use std::any::Any;

use crate::node::{Frame, FrameData, FrameMeta, Node, NodeError, PixelFormat, Result};

pub struct Resize {
    target_width: u32,
    target_height: u32,
}

impl Resize {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            target_width: width,
            target_height: height,
        }
    }
}

impl Node for Resize {
    fn process(&self, frame: Frame) -> Result<Frame> {
        let fmt = frame.meta.format;
        let (data, new_meta) = match frame.data {
            FrameData::Rgba(d) => {
                let img = image::RgbaImage::from_raw(frame.meta.width, frame.meta.height, d)
                    .ok_or_else(|| NodeError::UnsupportedFormat("rgba".to_string()))?;
                let resized = image::imageops::resize(
                    &img,
                    self.target_width,
                    self.target_height,
                    image::imageops::FilterType::Triangle,
                );
                let (w, h) = resized.dimensions();
                (
                    FrameData::Rgba(resized.into_raw()),
                    FrameMeta {
                        width: w,
                        height: h,
                        timestamp_ms: frame.meta.timestamp_ms,
                        format: PixelFormat::Rgba,
                    },
                )
            }
            FrameData::Rgb(d) => {
                let img = image::RgbImage::from_raw(frame.meta.width, frame.meta.height, d)
                    .ok_or_else(|| NodeError::UnsupportedFormat("rgb".to_string()))?;
                let resized = image::imageops::resize(
                    &img,
                    self.target_width,
                    self.target_height,
                    image::imageops::FilterType::Triangle,
                );
                let (w, h) = resized.dimensions();
                (
                    FrameData::Rgb(resized.into_raw()),
                    FrameMeta {
                        width: w,
                        height: h,
                        timestamp_ms: frame.meta.timestamp_ms,
                        format: PixelFormat::Rgb,
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
