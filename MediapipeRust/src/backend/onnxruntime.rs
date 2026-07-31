use crate::backend::{Backend, InferenceBackend, Model, Session, SessionBackend, Tensor, TensorInfo, TensorType, Error};

pub struct OnnxRuntimeBackend;

impl OnnxRuntimeBackend {
    pub fn new() -> Self {
        Self
    }
}

impl Default for OnnxRuntimeBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl Backend for OnnxRuntimeBackend {
    fn name(&self) -> &str {
        "onnxruntime"
    }
}

pub struct OnnxModel {
    pub model: Model,
    session: ort::session::Session,
}

impl OnnxModel {
    pub fn run(&mut self, input_data: &[f32], input_shape: &[usize]) -> Result<Vec<f32>, Error> {
        let input_tensor: ort::value::Tensor<f32> = ort::value::Tensor::from_array((
            input_shape.iter().map(|&s| s as i64).collect::<Vec<_>>(),
            input_data.iter().cloned().collect::<Vec<_>>().into_boxed_slice(),
        ))
        .map_err(|e| Error::Inference(format!("Failed to create input tensor: {}", e)))?;

        let outputs: ort::session::SessionOutputs = self.session
            .run(ort::inputs![input_tensor])
            .map_err(|e| Error::Inference(format!("ONNX inference failed: {}", e)))?;

        let output_tensor = &outputs[0];
        let (shape, data) = output_tensor
            .try_extract_tensor::<f32>()
            .map_err(|e| Error::Inference(format!("Failed to extract output tensor: {}", e)))?;

        let output_size: usize = shape.iter().map(|&s| s as usize).product();
        Ok(data[..output_size].to_vec())
    }
}

impl InferenceBackend for OnnxRuntimeBackend {
    fn load_model(&self, data: &[u8]) -> Result<Model, Error> {
        if data.is_empty() {
            return Err(Error::Model("Model data is empty".into()));
        }

        let session = ort::session::Session::builder()
            .map_err(|e| Error::Backend(format!("Failed to create session builder: {}", e)))?
            .commit_from_memory(data)
            .map_err(|e| Error::Backend(format!("Failed to load model: {}", e)))?;

        let inputs: Vec<TensorInfo> = session
            .inputs()
            .iter()
            .map(|input| {
                let name = input.name().to_string();
                let tensor_type = TensorType::F32;
                TensorInfo::new(name, vec![1, 3, 224, 224], tensor_type)
            })
            .collect();

        let outputs: Vec<TensorInfo> = session
            .outputs()
            .iter()
            .map(|output| {
                let name = output.name().to_string();
                let tensor_type = TensorType::F32;
                TensorInfo::new(name, vec![1, 1000], tensor_type)
            })
            .collect();

        Ok(Model { inputs, outputs })
    }

    fn create_session(&self, model: &Model) -> Result<Session, Error> {
        Err(Error::NotImplemented(
            "ONNX Runtime: session is created during load_model, use load_model_and_session instead".into(),
        ))
    }

    fn load_model_and_session(&self, data: &[u8]) -> Result<(Model, Session), Error> {
        if data.is_empty() {
            return Err(Error::Model("Model data is empty".into()));
        }

        let session = ort::session::Session::builder()
            .map_err(|e| Error::Backend(format!("Failed to create session builder: {}", e)))?
            .commit_from_memory(data)
            .map_err(|e| Error::Backend(format!("Failed to load model: {}", e)))?;

        let inputs: Vec<TensorInfo> = session
            .inputs()
            .iter()
            .map(|input| {
                let name = input.name().to_string();
                let tensor_type = TensorType::F32;
                TensorInfo::new(name, vec![1, 3, 224, 224], tensor_type)
            })
            .collect();

        let outputs: Vec<TensorInfo> = session
            .outputs()
            .iter()
            .map(|output| {
                let name = output.name().to_string();
                let tensor_type = TensorType::F32;
                TensorInfo::new(name, vec![1, 1000], tensor_type)
            })
            .collect();

        let model = Model { inputs, outputs };

        let onnx_session = OnnxSession { session };
        Ok((model, Session::OnnxRuntime(onnx_session)))
    }
}

pub struct OnnxSession {
    session: ort::session::Session,
}

impl OnnxSession {
    pub fn run(&mut self, input_data: &[f32], input_shape: &[usize]) -> Result<Vec<f32>, Error> {
        let input_tensor: ort::value::Tensor<f32> = ort::value::Tensor::from_array((
            input_shape.iter().map(|&s| s as i64).collect::<Vec<_>>(),
            input_data.iter().cloned().collect::<Vec<_>>().into_boxed_slice(),
        ))
        .map_err(|e| Error::Inference(format!("Failed to create input tensor: {}", e)))?;

        let outputs: ort::session::SessionOutputs = self.session
            .run(ort::inputs![input_tensor])
            .map_err(|e| Error::Inference(format!("ONNX inference failed: {}", e)))?;

        let output_tensor = &outputs[0];
        let (shape, data) = output_tensor
            .try_extract_tensor::<f32>()
            .map_err(|e| Error::Inference(format!("Failed to extract output tensor: {}", e)))?;

        let output_size: usize = shape.iter().map(|&s| s as usize).product();
        Ok(data[..output_size].to_vec())
    }
}

impl SessionBackend for OnnxSession {
    fn set_input(&mut self, _index: usize, _tensor: &Tensor) -> Result<(), Error> {
        Err(Error::NotImplemented("OnnxRuntime: use OnnxSession::run() instead".into()))
    }

    fn compute(&mut self) -> Result<(), Error> {
        Err(Error::NotImplemented("OnnxRuntime: use OnnxSession::run() instead".into()))
    }

    fn get_output(&mut self, _index: usize, _tensor: &mut Tensor) -> Result<(), Error> {
        Err(Error::NotImplemented("OnnxRuntime: use OnnxSession::run() instead".into()))
    }
}
