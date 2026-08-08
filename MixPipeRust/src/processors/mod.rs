use crate::node::{Frame, Node, Result};
use std::any::Any;

pub struct Resize {
    _target_width: u32,
    _target_height: u32,
}

impl Resize {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            _target_width: width,
            _target_height: height,
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
    _mean: [f32; 3],
    _std: [f32; 3],
}

impl Normalize {
    pub fn new(mean: [f32; 3], std: [f32; 3]) -> Self {
        Self { _mean: mean, _std: std }
    }
    pub fn imagenet() -> Self {
        Self {
            _mean: [123.675, 116.28, 103.53],
            _std: [58.395, 57.12, 57.375],
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
