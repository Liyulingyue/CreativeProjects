use crate::node::{Frame, Node, Result};
use std::any::Any;

pub struct OnnxModel;

impl OnnxModel {
    pub fn from_file(_path: &str) -> crate::node::Result<Self> {
        Ok(Self)
    }
}

impl Node for OnnxModel {
    fn process(&self, frame: Frame) -> Result<Frame> {
        Ok(frame)
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}
