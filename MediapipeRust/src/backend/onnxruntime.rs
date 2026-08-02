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

impl InferenceBackend for OnnxRuntimeBackend {
    fn load_model(&self, data: &[u8]) -> Result<Model, Error> {
        let (model, _) = self.load_model_and_session(data)?;
        Ok(model)
    }

    fn create_session(&self, _model: &Model) -> Result<Session, Error> {
        Err(Error::NotImplemented(
            "ONNX Runtime: use load_model_and_session() instead".into(),
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
                TensorInfo::new(name, vec![1, 224, 224, 3], tensor_type)
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

        let onnx_session = OnnxSession::new(session);
        Ok((model, Session::OnnxRuntime(onnx_session)))
    }
}

pub struct OnnxSession {
    session: ort::session::Session,
    input_data: Option<Vec<f32>>,
    input_shape: Option<Vec<usize>>,
    output_data: Option<Vec<f32>>,
}

impl OnnxSession {
    fn new(session: ort::session::Session) -> Self {
        Self {
            session,
            input_data: None,
            input_shape: None,
            output_data: None,
        }
    }

    fn run_inference(&mut self) -> Result<(), Error> {
        let input_data = self.input_data.take()
            .ok_or_else(|| Error::Inference("No input data set".into()))?;
        let input_shape = self.input_shape.take()
            .ok_or_else(|| Error::Inference("No input shape set".into()))?;

        let input_tensor: ort::value::Tensor<f32> = ort::value::Tensor::from_array((
            input_shape.iter().map(|&s| s as i64).collect::<Vec<_>>(),
            input_data.into_boxed_slice(),
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
        self.output_data = Some(data[..output_size].to_vec());

        Ok(())
    }
}

impl SessionBackend for OnnxSession {
    fn set_input(&mut self, _index: usize, tensor: &Tensor) -> Result<(), Error> {
        if tensor.tensor_type != TensorType::F32 {
            return Err(Error::Inference("Only F32 tensors are supported".into()));
        }
        let data: Vec<f32> = tensor.data
            .chunks(4)
            .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
            .collect();
        self.input_data = Some(data);
        self.input_shape = Some(tensor.shape.clone());
        Ok(())
    }

    fn compute(&mut self) -> Result<(), Error> {
        self.run_inference()
    }

    fn get_output(&mut self, _index: usize, tensor: &mut Tensor) -> Result<(), Error> {
        if let Some(ref data) = self.output_data {
            let bytes: Vec<u8> = data.iter()
                .flat_map(|&f| f.to_le_bytes().to_vec())
                .collect();
            tensor.data = bytes;
            Ok(())
        } else {
            Err(Error::Inference("No output available".into()))
        }
    }
}
