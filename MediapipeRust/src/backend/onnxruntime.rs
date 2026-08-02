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
                let dtype = input.dtype();
                let tensor_type = match dtype.tensor_type() {
                    Some(ort::value::TensorElementType::Float32) => TensorType::F32,
                    Some(ort::value::TensorElementType::Uint8) => TensorType::U8,
                    Some(ort::value::TensorElementType::Int8) => TensorType::I32,
                    Some(ort::value::TensorElementType::Int32) => TensorType::I32,
                    Some(_) => TensorType::F32,
                    None => TensorType::F32,
                };
                let shape: Vec<usize> = dtype
                    .tensor_shape()
                    .map(|s| s.iter().map(|&d| d as usize).collect())
                    .unwrap_or_else(|| vec![1, 224, 224, 3]);
                TensorInfo::new(name, shape, tensor_type)
            })
            .collect();

        let outputs: Vec<TensorInfo> = session
            .outputs()
            .iter()
            .map(|output| {
                let name = output.name().to_string();
                let dtype = output.dtype();
                let tensor_type = match dtype.tensor_type() {
                    Some(ort::value::TensorElementType::Float32) => TensorType::F32,
                    Some(ort::value::TensorElementType::Uint8) => TensorType::U8,
                    _ => TensorType::F32,
                };
                let shape: Vec<usize> = dtype
                    .tensor_shape()
                    .map(|s| s.iter().map(|&d| d as usize).collect())
                    .unwrap_or_else(|| vec![1, 1000]);
                TensorInfo::new(name, shape, tensor_type)
            })
            .collect();

        let model = Model { inputs, outputs };

        let onnx_session = OnnxSession::new(session);
        Ok((model, Session::OnnxRuntime(onnx_session)))
    }
}

pub struct OnnxSession {
    session: ort::session::Session,
    input_data: Option<InputData>,
    input_shape: Option<Vec<usize>>,
    output_data: Vec<Vec<f32>>,
    output_index: usize,
}

enum InputData {
    F32(Vec<f32>),
    U8(Vec<u8>),
}

impl OnnxSession {
    fn new(session: ort::session::Session) -> Self {
        Self {
            session,
            input_data: None,
            input_shape: None,
            output_data: Vec::new(),
            output_index: 0,
        }
    }

    fn run_inference(&mut self) -> Result<(), Error> {
        let input_data = self.input_data.take()
            .ok_or_else(|| Error::Inference("No input data set".into()))?;
        let input_shape = self.input_shape.take()
            .ok_or_else(|| Error::Inference("No input shape set".into()))?;

        let outputs: ort::session::SessionOutputs = match input_data {
            InputData::F32(data) => {
                let input_tensor: ort::value::Tensor<f32> = ort::value::Tensor::from_array((
                    input_shape.iter().map(|&s| s as i64).collect::<Vec<_>>(),
                    data.into_boxed_slice(),
                ))
                .map_err(|e| Error::Inference(format!("Failed to create tensor: {}", e)))?;

                self.session
                    .run(ort::inputs![input_tensor])
                    .map_err(|e| Error::Inference(format!("ONNX inference failed: {}", e)))?
            }
            InputData::U8(data) => {
                let input_tensor: ort::value::Tensor<u8> = ort::value::Tensor::from_array((
                    input_shape.iter().map(|&s| s as i64).collect::<Vec<_>>(),
                    data.into_boxed_slice(),
                ))
                .map_err(|e| Error::Inference(format!("Failed to create tensor: {}", e)))?;

                self.session
                    .run(ort::inputs![input_tensor])
                    .map_err(|e| Error::Inference(format!("ONNX inference failed: {}", e)))?
            }
        };

        self.output_data = (0..outputs.len())
            .filter_map(|i| {
                let output = &outputs[i];
                let (shape, data) = output.try_extract_tensor::<f32>().ok()?;
                let output_size: usize = shape.iter().map(|&s| s as usize).product();
                Some(data[..output_size].to_vec())
            })
            .collect();
        self.output_index = 0;

        Ok(())
    }
}

impl SessionBackend for OnnxSession {
    fn set_input(&mut self, _index: usize, tensor: &Tensor) -> Result<(), Error> {
        let data = match tensor.tensor_type {
            TensorType::F32 => {
                let f32_data: Vec<f32> = tensor.data
                    .chunks(4)
                    .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
                    .collect();
                InputData::F32(f32_data)
            }
            TensorType::U8 => {
                InputData::U8(tensor.data.clone())
            }
            _ => {
                return Err(Error::Inference(format!("Unsupported input type: {:?}", tensor.tensor_type)));
            }
        };

        self.input_data = Some(data);
        self.input_shape = Some(tensor.shape.clone());
        Ok(())
    }

    fn compute(&mut self) -> Result<(), Error> {
        self.run_inference()
    }

    fn get_output(&mut self, index: usize, tensor: &mut Tensor) -> Result<(), Error> {
        if index < self.output_data.len() {
            let data = self.output_data[index].clone();
            let bytes: Vec<u8> = data.iter()
                .flat_map(|&f| f.to_le_bytes().to_vec())
                .collect();
            tensor.data = bytes;
            tensor.tensor_type = TensorType::F32;
            Ok(())
        } else {
            Err(Error::Inference(format!("Output index {} out of range", index)).into())
        }
    }
}
