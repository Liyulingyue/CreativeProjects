use std::any::Any;

use crate::node::{Frame, Node, NodeError, Result};

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
