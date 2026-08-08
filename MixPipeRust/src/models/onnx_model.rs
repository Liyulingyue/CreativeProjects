use std::any::Any;
use std::path::Path;

use crate::node::{Frame, FrameData, Node, NodeError, Result};

pub struct OnnxModel {
    session: ort::session::Session,
    input_names: Vec<String>,
    output_names: Vec<String>,
}

impl OnnxModel {
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let session = ort::session::Session::builder()
            .map_err(|e| NodeError::Model(format!("build session: {}", e)))?
            .commit_from_file(path.as_ref())
            .map_err(|e| NodeError::Model(format!("load model: {}", e)))?;

        let input_names: Vec<String> = session
            .inputs()
            .iter()
            .map(|i| i.name().to_string())
            .collect();

        let output_names: Vec<String> = session
            .outputs()
            .iter()
            .map(|o| o.name().to_string())
            .collect();

        Ok(Self {
            session,
            input_names,
            output_names,
        })
    }

    pub fn run(&self, input: ort::value::Value) -> Result<Vec<ort::value::Value>> {
        let inputs = ort::inputs![input].map_err(|e| NodeError::Model(e.to_string()))?;
        self.session
            .run(inputs)
            .map_err(|e| NodeError::Model(format!("inference: {}", e)))
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
