use std::any::Any;
use std::sync::Arc;

use crate::node::{Frame, Node, NodeError, Result};

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
