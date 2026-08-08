use std::any::Any;

use crate::node::{Frame, FrameData, FrameMeta, Node, NodeError, PixelFormat, Result};

pub struct ResizeToFit {
    target_width: u32,
    target_height: u32,
    fill_color: [u8; 3],
}

impl ResizeToFit {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            target_width: width,
            target_height: height,
            fill_color: [114, 114, 114],
        }
    }

    pub fn with_fill(mut self, color: [u8; 3]) -> Self {
        self.fill_color = color;
        self
    }
}

impl Node for ResizeToFit {
    fn process(&self, frame: Frame) -> Result<Frame> {
        let fmt = frame.meta.format;
        let (data, new_meta) = match frame.data {
            FrameData::Rgb(d) => {
                let img =
                    image::RgbImage::from_raw(frame.meta.width, frame.meta.height, d)
                        .ok_or_else(|| NodeError::UnsupportedFormat("resize_to_fit".to_string()))?;

                let scale = (self.target_width as f32 / frame.meta.width as f32)
                    .min(self.target_height as f32 / frame.meta.height as f32);

                let new_w = (frame.meta.width as f32 * scale) as u32;
                let new_h = (frame.meta.height as f32 * scale) as u32;

                let resized = image::imageops::resize(
                    &img,
                    new_w,
                    new_h,
                    image::imageops::FilterType::Triangle,
                );

                let mut padded = image::RgbImage::from_pixel(
                    self.target_width,
                    self.target_height,
                    image::Rgb(self.fill_color),
                );
                let offset_x = (self.target_width - new_w) / 2;
                let offset_y = (self.target_height - new_h) / 2;
                image::imageops::overlay(&mut padded, &resized, offset_x as i64, offset_y as i64);

                (
                    FrameData::Rgb(padded.into_raw()),
                    FrameMeta {
                        width: self.target_width,
                        height: self.target_height,
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
