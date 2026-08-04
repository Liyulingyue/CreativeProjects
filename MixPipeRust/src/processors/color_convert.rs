use std::any::Any;

use crate::node::{Frame, FrameData, FrameMeta, Node, NodeError, PixelFormat, Result};

pub struct ColorConvert {
    target_format: PixelFormat,
}

impl ColorConvert {
    pub fn new(target: PixelFormat) -> Self {
        Self {
            target_format: target,
        }
    }
}

impl Node for ColorConvert {
    fn process(&self, frame: Frame) -> Result<Frame> {
        let converted = match (frame.data, &frame.meta.format, &self.target_format) {
            (FrameData::Rgba(d), PixelFormat::Rgba, PixelFormat::Rgb) => {
                let img = image::RgbaImage::from_raw(frame.meta.width, frame.meta.height, d)
                    .ok_or_else(|| NodeError::UnsupportedFormat("rgba to rgb".to_string()))?;
                let rgb = image::DynamicImage::ImageRgba8(img).to_rgb8();
                FrameData::Rgb(rgb.into_raw())
            }
            (FrameData::Rgb(d), PixelFormat::Rgb, PixelFormat::Rgba) => {
                let img = image::RgbImage::from_raw(frame.meta.width, frame.meta.height, d)
                    .ok_or_else(|| NodeError::UnsupportedFormat("rgb to rgba".to_string()))?;
                let rgba = image::DynamicImage::ImageRgb8(img).to_rgba8();
                FrameData::Rgba(rgba.into_raw())
            }
            _ => {
                return Err(NodeError::UnsupportedFormat(format!(
                    "{:?} -> {:?}",
                    frame.meta.format, self.target_format
                )))
            }
        };

        let mut meta = frame.meta;
        meta.format = self.target_format;
        Ok(Frame::new(converted, meta))
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}
