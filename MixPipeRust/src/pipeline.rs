use crate::node::{Frame, Node, Result};

pub struct Pipeline {
    nodes: Vec<Box<dyn Node>>,
}

impl Pipeline {
    pub fn run(&self, frame: Frame) -> Result<Frame> {
        let mut current = frame;
        for node in &self.nodes {
            current = node.process(current)?;
        }
        Ok(current)
    }

    pub fn nodes(&self) -> usize {
        self.nodes.len()
    }
}

pub struct PipelineBuilder {
    nodes: Vec<Box<dyn Node>>,
}

impl PipelineBuilder {
    pub fn new() -> Self {
        Self { nodes: Vec::new() }
    }

    pub fn add_node<N: Node + 'static>(mut self, node: N) -> Self {
        self.nodes.push(Box::new(node));
        self
    }

    pub fn build(self) -> Pipeline {
        Pipeline { nodes: self.nodes }
    }
}

impl Default for PipelineBuilder {
    fn default() -> Self {
        Self::new()
    }
}
