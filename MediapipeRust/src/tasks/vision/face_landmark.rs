use crate::backend::{Class, InferenceBackend, Landmark, Model, Session, Tensor, TensorType, Error};
use std::sync::Arc;

pub struct FaceLandmarkerBuilder<B: InferenceBackend> {
    backend: B,
    options: FaceLandmarkerOptions,
}

#[derive(Clone, Debug, Default)]
pub struct FaceLandmarkerOptions {
    pub num_faces: u32,
    pub min_face_detection_confidence: f32,
    pub min_face_presence_confidence: f32,
    pub min_tracking_confidence: f32,
    pub output_face_blendshapes: bool,
}

impl<B: InferenceBackend> FaceLandmarkerBuilder<B> {
    pub fn new(backend: B) -> Self {
        Self {
            backend,
            options: FaceLandmarkerOptions::default(),
        }
    }

    pub fn num_faces(mut self, num: u32) -> Self {
        self.options.num_faces = num;
        self
    }

    pub fn min_face_detection_confidence(mut self, conf: f32) -> Self {
        self.options.min_face_detection_confidence = conf;
        self
    }

    pub fn min_tracking_confidence(mut self, conf: f32) -> Self {
        self.options.min_tracking_confidence = conf;
        self
    }

    pub fn output_face_blendshapes(mut self, output: bool) -> Self {
        self.options.output_face_blendshapes = output;
        self
    }

    pub fn build_from_file(self, path: &str) -> Result<FaceLandmarker<B>, Error> {
        let data = std::fs::read(path)?;
        self.build_from_buffer(data)
    }

    pub fn build_from_buffer(self, buffer: Vec<u8>) -> Result<FaceLandmarker<B>, Error> {
        let model = self.backend.load_model(&buffer)?;
        Ok(FaceLandmarker {
            backend: self.backend,
            model: Arc::new(model),
            options: self.options,
        })
    }
}

pub struct FaceLandmarker<B: InferenceBackend> {
    backend: B,
    model: Arc<Model>,
    options: FaceLandmarkerOptions,
}

impl<B: InferenceBackend> FaceLandmarker<B> {
    pub fn new_session(&self) -> Result<FaceLandmarkerSession<'_, B>, Error> {
        let session = self.backend.create_session(&*self.model)?;
        Ok(FaceLandmarkerSession {
            landmarker: self,
            session,
        })
    }

    pub fn detect(&self, image_data: &[u8], width: u32, height: u32) -> Result<Vec<FaceLandmarkResult>, Error> {
        let mut session = self.new_session()?;
        session.detect(image_data, width, height)
    }
}

pub struct FaceLandmarkerSession<'a, B: InferenceBackend> {
    landmarker: &'a FaceLandmarker<B>,
    session: Session,
}

#[derive(Debug, Clone)]
pub struct FaceLandmarkResult {
    pub landmarks: Vec<Landmark>,
    pub blendshapes: Option<Vec<Class>>,
    pub bounding_box: Option<crate::backend::BoundingBox>,
}

impl<'a, B: InferenceBackend> FaceLandmarkerSession<'a, B> {
    pub fn detect(&mut self, image_data: &[u8], width: u32, height: u32) -> Result<Vec<FaceLandmarkResult>, Error> {
        let input_tensor = Tensor::new(
            self.landmarker.model.inputs[0].tensor_type,
            self.landmarker.model.inputs[0].shape.clone(),
            image_data.to_vec(),
        );
        self.session.set_input(0, &input_tensor)?;
        self.session.compute()?;

        let mut landmarks_tensor = Tensor::empty(TensorType::F32, self.landmarker.model.outputs[0].shape.clone());
        self.session.get_output(0, &mut landmarks_tensor)?;

        let landmarks_data = landmarks_tensor.as_f32();
        let num_landmarks = landmarks_data.len() / 3;
        let mut landmarks = Vec::with_capacity(num_landmarks);

        for i in 0..num_landmarks {
            landmarks.push(Landmark::new(
                landmarks_data[i * 3],
                landmarks_data[i * 3 + 1],
                landmarks_data[i * 3 + 2],
            ));
        }

        let mut result = FaceLandmarkResult {
            landmarks,
            blendshapes: None,
            bounding_box: None,
        };

        if self.landmarker.options.output_face_blendshapes && self.landmarker.model.outputs.len() > 1 {
            let mut blendshapes_tensor = Tensor::empty(TensorType::F32, self.landmarker.model.outputs[1].shape.clone());
            self.session.get_output(1, &mut blendshapes_tensor)?;
            let blendshapes_data = blendshapes_tensor.as_f32();
            let blendshapes: Vec<Class> = blendshapes_data
                .iter()
                .enumerate()
                .map(|(i, &s)| Class::new(i as i32, s, format!("blendshape_{}", i)))
                .collect();
            result.blendshapes = Some(blendshapes);
        }

        Ok(vec![result])
    }
}
