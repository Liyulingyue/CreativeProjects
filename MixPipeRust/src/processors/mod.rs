use crate::node::{Frame, Node, Result};
use std::any::Any;

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
        Ok(frame)
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

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
        Ok(frame)
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

pub struct ColorConvert;

impl ColorConvert {
    pub fn new() -> Self {
        Self
    }
}

impl Default for ColorConvert {
    fn default() -> Self {
        Self::new()
    }
}

impl Node for ColorConvert {
    fn process(&self, frame: Frame) -> Result<Frame> {
        Ok(frame)
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}
