use crate::backend::{Backend, InferenceBackend, Model, Session, SessionBackend, Tensor, TensorInfo, TensorType, Error};

pub struct NativeBackend {
    is_mock: bool,
    mock_inputs: Vec<TensorInfo>,
    mock_outputs: Vec<TensorInfo>,
}

impl NativeBackend {
    pub fn new() -> Self {
        Self {
            is_mock: false,
            mock_inputs: vec![TensorInfo::new("input", vec![1, 224, 224, 3], TensorType::F32)],
            mock_outputs: vec![TensorInfo::new("output", vec![1, 1000], TensorType::F32)],
        }
    }

    pub fn mock() -> Self {
        Self {
            is_mock: true,
            mock_inputs: vec![TensorInfo::new("input", vec![1, 256, 256, 3], TensorType::F32)],
            mock_outputs: vec![TensorInfo::new("output", vec![1, 1000], TensorType::F32)],
        }
    }

    pub fn mock_detection() -> Self {
        Self {
            is_mock: true,
            mock_inputs: vec![TensorInfo::new("input", vec![1, 256, 256, 3], TensorType::F32)],
            mock_outputs: vec![
                TensorInfo::new("boxes", vec![1, 4], TensorType::F32),
                TensorInfo::new("scores", vec![1, 1], TensorType::F32),
            ],
        }
    }

    pub fn mock_landmark() -> Self {
        Self {
            is_mock: true,
            mock_inputs: vec![TensorInfo::new("input", vec![1, 256, 256, 3], TensorType::F32)],
            mock_outputs: vec![
                TensorInfo::new("landmarks", vec![1, 21, 3], TensorType::F32),
                TensorInfo::new("world_landmarks", vec![1, 21, 3], TensorType::F32),
                TensorInfo::new("handedness", vec![1, 1], TensorType::F32),
            ],
        }
    }
}

impl Default for NativeBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl Backend for NativeBackend {
    fn name(&self) -> &str {
        if self.is_mock { "native-mock" } else { "native-tflite" }
    }
}

impl InferenceBackend for NativeBackend {
    fn load_model(&self, data: &[u8]) -> Result<Model, Error> {
        if self.is_mock || data.len() < 8 {
            return Ok(Model {
                inputs: self.mock_inputs.clone(),
                outputs: self.mock_outputs.clone(),
            });
        }

        super::tflite::TFLiteModelParser.parse(data)
    }

    fn create_session(&self, model: &Model) -> Result<Session, Error> {
        Ok(Session::Native(NativeSession::with_model(model.clone())))
    }
}

#[derive(Debug)]
pub struct NativeSession {
    model: Option<Model>,
    input_buffer: Vec<u8>,
    output_buffers: Vec<Vec<u8>>,
}

impl NativeSession {
    pub fn new() -> Self {
        Self {
            model: None,
            input_buffer: vec![],
            output_buffers: vec![],
        }
    }

    pub fn with_model(model: Model) -> Self {
        let input_size: usize = model.inputs.iter().map(|i| {
            i.shape.iter().product::<usize>() * i.tensor_type.byte_size()
        }).sum();

        let output_buffers: Vec<Vec<u8>> = model.outputs.iter().map(|o| {
            let size = o.shape.iter().product::<usize>() * o.tensor_type.byte_size();
            vec![0u8; size]
        }).collect();

        Self {
            model: Some(model),
            input_buffer: vec![0u8; input_size],
            output_buffers,
        }
    }
}

impl Default for NativeSession {
    fn default() -> Self {
        Self::new()
    }
}

impl SessionBackend for NativeSession {
    fn set_input(&mut self, index: usize, tensor: &Tensor) -> Result<(), Error> {
        if index != 0 {
            return Err(Error::InvalidArgument(format!("Invalid input index: {}", index)));
        }
        self.input_buffer = tensor.data.clone();
        Ok(())
    }

    fn compute(&mut self) -> Result<(), Error> {
        for buffer in &mut self.output_buffers {
            if buffer.is_empty() {
                *buffer = vec![0u8; 1000 * 4];
            }
        }
        Ok(())
    }

    fn get_output(&mut self, index: usize, tensor: &mut Tensor) -> Result<(), Error> {
        if index >= self.output_buffers.len() {
            return Err(Error::InvalidArgument(format!("Invalid output index: {}", index)));
        }
        tensor.data = self.output_buffers[index].clone();
        Ok(())
    }
}
