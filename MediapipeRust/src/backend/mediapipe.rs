use crate::backend::{Backend, InferenceBackend, Model, Error, TensorInfo, TensorType, Session};

pub struct MediaPipeBackend {
    lib_path: Option<String>,
}

impl MediaPipeBackend {
    pub fn new() -> Self {
        Self { lib_path: None }
    }

    pub fn with_library(path: impl Into<String>) -> Self {
        Self { lib_path: Some(path.into()) }
    }
}

impl Default for MediaPipeBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl Backend for MediaPipeBackend {
    fn name(&self) -> &str {
        "mediapipe-cpp"
    }
}

impl InferenceBackend for MediaPipeBackend {
    fn load_model(&self, _data: &[u8]) -> Result<Model, Error> {
        if self.lib_path.is_none() {
            return Err(Error::Backend(
                "MediaPipe library path not set. Use MediaPipeBackend::with_library()".into(),
            ));
        }
        unsafe { self.load_model_impl() }
    }

    fn create_session(&self, model: &Model) -> Result<Session, Error> {
        if self.lib_path.is_none() {
            return Err(Error::Backend(
                "MediaPipe library path not set. Use MediaPipeBackend::with_library()".into(),
            ));
        }
        unsafe { self.create_session_impl(model) }
    }
}

impl MediaPipeBackend {
    unsafe fn load_model_impl(&self) -> Result<Model, Error> {
        extern "C" {
            fn mp_load_model(model_path: *const libc::c_char, graph: *mut *mut MP_Graph) -> i32;
            fn mp_delete_graph(graph: *mut MP_Graph);
        }

        let path = self.lib_path.as_ref().unwrap();
        let c_path = std::ffi::CString::new(path.as_str())
            .map_err(|e| Error::Backend(format!("Invalid path: {}", e)))?;

        let mut graph: *mut MP_Graph = std::ptr::null_mut();
        let status = mp_load_model(c_path.as_ptr(), &mut graph);

        if status != 0 {
            return Err(Error::Backend(format!("Failed to load model: error code {}", status)));
        }

        if !graph.is_null() {
            mp_delete_graph(graph);
        }

        Ok(Model {
            inputs: vec![TensorInfo::new("input", vec![1, 224, 224, 3], TensorType::F32)],
            outputs: vec![TensorInfo::new("output", vec![1, 1000], TensorType::F32)],
        })
    }

    unsafe fn create_session_impl(&self, _model: &Model) -> Result<Session, Error> {
        extern "C" {
            fn mp_create_session(graph: *mut MP_Graph, config: *const libc::c_char, session: *mut *mut MP_Session) -> i32;
            fn mp_delete_session(session: *mut MP_Session);
        }

        let mut session: *mut MP_Session = std::ptr::null_mut();
        let config = std::ffi::CString::new("").unwrap();

        let status = mp_create_session(std::ptr::null_mut(), config.as_ptr(), &mut session);

        if status != 0 {
            return Err(Error::Backend(format!("Failed to create session: error code {}", status)));
        }

        Ok(Session::MediaPipeCpp(MediaPipeSession {
            session,
            _marker: std::marker::PhantomData,
        }))
    }
}

#[repr(C)]
pub struct MP_Graph {
    _priv: [u8; 0],
}

#[repr(C)]
pub struct MP_Session {
    _priv: [u8; 0],
}

#[repr(C)]
pub struct MP_Tensor {
    _priv: [u8; 0],
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct MP_Landmark {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub visibility: f32,
    pub presence: f32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct MP_LandmarkResult {
    pub landmarks: *mut MP_Landmark,
    pub num_landmarks: i32,
    pub handedness: i32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct MP_BoundingBox {
    pub x_min: f32,
    pub y_min: f32,
    pub width: f32,
    pub height: f32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct MP_Category {
    pub class_id: i32,
    pub score: f32,
    pub label: *const libc::c_char,
    pub display_name: *const libc::c_char,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct MP_DetectionResult {
    pub bounding_box: MP_BoundingBox,
    pub categories: *mut MP_Category,
    pub num_categories: i32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct MP_TensorInfo {
    pub name: *const libc::c_char,
    pub shape: *mut i32,
    pub shape_size: i32,
    pub type_: i32,
}

pub struct MediaPipeSession<B> {
    session: *mut MP_Session,
    _marker: std::marker::PhantomData<B>,
}

impl<B> Drop for MediaPipeSession<B> {
    fn drop(&mut self) {
        if !self.session.is_null() {
            unsafe {
                extern "C" {
                    fn mp_delete_session(session: *mut MP_Session);
                }
                mp_delete_session(self.session);
            }
        }
    }
}
