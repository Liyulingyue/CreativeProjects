pub mod native;
#[cfg(feature = "onnxruntime")]
pub mod onnxruntime;
#[cfg(feature = "mediapipe-cpp")]
pub mod mediapipe;
pub mod tflite;

pub use native::NativeBackend;
#[cfg(feature = "onnxruntime")]
pub use onnxruntime::OnnxRuntimeBackend;
#[cfg(feature = "mediapipe-cpp")]
pub use mediapipe::MediaPipeBackend;

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Backend error: {0}")]
    Backend(String),
    #[error("Model error: {0}")]
    Model(String),
    #[error("Inference error: {0}")]
    Inference(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Not implemented: {0}")]
    NotImplemented(String),
    #[error("Invalid argument: {0}")]
    InvalidArgument(String),
}

#[derive(Clone, Debug, Copy, PartialEq, Eq)]
pub enum TensorType {
    F32,
    U8,
    I32,
    F16,
}

impl TensorType {
    pub fn byte_size(&self) -> usize {
        match self {
            TensorType::F32 => 4,
            TensorType::U8 => 1,
            TensorType::I32 => 4,
            TensorType::F16 => 2,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Tensor {
    pub shape: Vec<usize>,
    pub tensor_type: TensorType,
    pub data: Vec<u8>,
}

impl Tensor {
    pub fn new(tensor_type: TensorType, shape: Vec<usize>, data: Vec<u8>) -> Self {
        Self { shape, tensor_type, data }
    }

    pub fn empty(tensor_type: TensorType, shape: Vec<usize>) -> Self {
        let size: usize = shape.iter().product();
        let data = vec![0u8; size * tensor_type.byte_size()];
        Self { shape, tensor_type, data }
    }

    pub fn as_f32(&self) -> &[f32] {
        assert_eq!(self.tensor_type, TensorType::F32);
        unsafe { std::slice::from_raw_parts(self.data.as_ptr() as *const f32, self.data.len() / 4) }
    }

    pub fn as_f32_mut(&mut self) -> &mut [f32] {
        assert_eq!(self.tensor_type, TensorType::F32);
        unsafe { std::slice::from_raw_parts_mut(self.data.as_ptr() as *mut f32, self.data.len() / 4) }
    }

    pub fn as_u8(&self) -> &[u8] {
        &self.data
    }

    pub fn as_u8_mut(&mut self) -> &mut [u8] {
        &mut self.data
    }
}

pub trait Backend: Send + Sync {
    fn name(&self) -> &str;
}

pub trait InferenceBackend: Backend + Send + Sync {
    fn load_model(&self, data: &[u8]) -> Result<Model, Error>;
    fn create_session(&self, model: &Model) -> Result<Session, Error>;
    fn load_model_and_session(&self, _data: &[u8]) -> Result<(Model, Session), Error> {
        Err(Error::NotImplemented("Backend does not support load_model_and_session".into()))
    }
}

pub trait SessionBackend: Send + Sync {
    fn set_input(&mut self, index: usize, tensor: &Tensor) -> Result<(), Error>;
    fn compute(&mut self) -> Result<(), Error>;
    fn get_output(&mut self, index: usize, tensor: &mut Tensor) -> Result<(), Error>;
}

#[derive(Clone, Debug)]
pub struct Model {
    pub inputs: Vec<TensorInfo>,
    pub outputs: Vec<TensorInfo>,
}

#[derive(Clone, Debug)]
pub struct TensorInfo {
    pub name: String,
    pub shape: Vec<usize>,
    pub tensor_type: TensorType,
}

impl TensorInfo {
    pub fn new(name: impl Into<String>, shape: Vec<usize>, tensor_type: TensorType) -> Self {
        Self { name: name.into(), shape, tensor_type }
    }
}

pub enum Session {
    Native(native::NativeSession),
    #[cfg(feature = "onnxruntime")]
    OnnxRuntime(onnxruntime::OnnxSession),
    #[cfg(feature = "mediapipe-cpp")]
    MediaPipeCpp(mediapipe::MediaPipeSession<()>),
}

impl Session {
    pub fn set_input(&mut self, index: usize, tensor: &Tensor) -> Result<(), Error> {
        match self {
            Session::Native(s) => s.set_input(index, tensor),
            #[cfg(feature = "onnxruntime")]
            Session::OnnxRuntime(s) => s.set_input(index, tensor),
            #[cfg(feature = "mediapipe-cpp")]
            Session::MediaPipeCpp(_) => Err(Error::NotImplemented("MediaPipeCpp session not implemented".into())),
        }
    }

    pub fn compute(&mut self) -> Result<(), Error> {
        match self {
            Session::Native(s) => s.compute(),
            #[cfg(feature = "onnxruntime")]
            Session::OnnxRuntime(s) => s.compute(),
            #[cfg(feature = "mediapipe-cpp")]
            Session::MediaPipeCpp(_) => Err(Error::NotImplemented("MediaPipeCpp session not implemented".into())),
        }
    }

    pub fn get_output(&mut self, index: usize, tensor: &mut Tensor) -> Result<(), Error> {
        match self {
            Session::Native(s) => s.get_output(index, tensor),
            #[cfg(feature = "onnxruntime")]
            Session::OnnxRuntime(s) => s.get_output(index, tensor),
            #[cfg(feature = "mediapipe-cpp")]
            Session::MediaPipeCpp(_) => Err(Error::NotImplemented("MediaPipeCpp session not implemented".into())),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Class {
    pub index: i32,
    pub score: f32,
    pub label: String,
    pub display_name: Option<String>,
}

impl Class {
    pub fn new(index: i32, score: f32, label: impl Into<String>) -> Self {
        Self { index, score, label: label.into(), display_name: None }
    }
}

#[derive(Debug, Clone)]
pub struct Detection {
    pub bounding_box: BoundingBox,
    pub categories: Vec<Class>,
}

#[derive(Debug, Clone)]
pub struct BoundingBox {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}

impl BoundingBox {
    pub fn new(left: f32, top: f32, right: f32, bottom: f32) -> Self {
        Self { left, top, right, bottom }
    }

    pub fn width(&self) -> f32 { self.right - self.left }
    pub fn height(&self) -> f32 { self.bottom - self.top }
    pub fn center(&self) -> (f32, f32) {
        ((self.left + self.right) / 2.0, (self.top + self.bottom) / 2.0)
    }
}

#[derive(Debug, Clone)]
pub struct Landmark {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub visibility: f32,
    pub presence: f32,
}

impl Landmark {
    pub fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z, visibility: 1.0, presence: 1.0 }
    }
}

#[derive(Debug, Clone)]
pub struct SegmentationMask {
    pub width: u32,
    pub height: u32,
    pub category_mask: Vec<u8>,
    pub confidence_mask: Option<Vec<f32>>,
}

#[derive(Debug, Clone)]
pub struct Embedding {
    pub values: Vec<f32>,
    pub label: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct CategoriesFilter {
    pub label_deny_list: Vec<String>,
    pub label_allow_list: Vec<String>,
    pub min_score: f32,
}

impl CategoriesFilter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn label_allow_list(mut self, labels: Vec<String>) -> Self {
        self.label_allow_list = labels;
        self
    }

    pub fn label_deny_list(mut self, labels: Vec<String>) -> Self {
        self.label_deny_list = labels;
        self
    }

    pub fn min_score(mut self, score: f32) -> Self {
        self.min_score = score;
        self
    }

    pub fn filter(&self, classes: &mut Vec<Class>) {
        classes.retain(|c| {
            if c.score < self.min_score {
                return false;
            }
            if !self.label_allow_list.is_empty() && !self.label_allow_list.contains(&c.label) {
                return false;
            }
            if self.label_deny_list.contains(&c.label) {
                return false;
            }
            true
        });
    }
}
