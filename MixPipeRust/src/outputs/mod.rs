use crate::node::{Frame, Node, Result};
use std::any::Any;
use std::sync::Arc;

pub type FrameCallback = Arc<dyn Fn(Frame) + Send + Sync>;

pub struct CallbackOutput {
    callback: FrameCallback,
}

impl CallbackOutput {
    pub fn new(callback: FrameCallback) -> Self {
        Self { callback }
    }
}

impl Node for CallbackOutput {
    fn process(&self, frame: Frame) -> Result<Frame> {
        (self.callback)(frame.clone());
        Ok(frame)
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

pub struct NullOutput;

impl NullOutput {
    pub fn new() -> Self {
        Self
    }
}

impl Default for NullOutput {
    fn default() -> Self {
        Self::new()
    }
}

impl Node for NullOutput {
    fn process(&self, frame: Frame) -> Result<Frame> {
        Ok(frame)
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}
