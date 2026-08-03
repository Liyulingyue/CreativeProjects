use crate::backend::{InferenceBackend, Landmark, Session, Tensor, TensorType, Error};

pub struct IrisTrackerBuilder;

impl IrisTrackerBuilder {
    pub fn new() -> Self {
        Self
    }

    pub fn build_from_file<B: InferenceBackend>(self, backend: &B, path: &str) -> Result<IrisTracker, Error> {
        let data = std::fs::read(path)?;
        self.build_from_buffer(backend, data)
    }

    pub fn build_from_buffer<B: InferenceBackend>(self, backend: &B, buffer: Vec<u8>) -> Result<IrisTracker, Error> {
        let (model, session) = backend.load_model_and_session(&buffer)?;
        Ok(IrisTracker {
            model,
            session,
        })
    }
}

pub struct IrisTracker {
    model: crate::backend::Model,
    session: Session,
}

#[derive(Debug, Clone)]
pub struct IrisResult {
    pub eyes_contours: Vec<Landmark>,
    pub iris: Vec<Landmark>,
}

impl IrisTracker {
    pub fn track(&mut self, image_data: &[u8], _width: u32, _height: u32) -> Result<IrisResult, Error> {
        let input_shape = &self.model.inputs[0].shape.clone();
        let input_type = self.model.inputs[0].tensor_type;

        let input_data = match input_type {
            TensorType::F32 => {
                let f32_data: Vec<f32> = image_data.iter()
                    .map(|&p| p as f32 / 255.0)
                    .collect();
                let bytes: Vec<u8> = f32_data.iter()
                    .flat_map(|&f| f.to_le_bytes())
                    .collect();
                bytes
            }
            _ => image_data.to_vec(),
        };

        let input_tensor = Tensor::new(input_type, input_shape.clone(), input_data);
        self.session.set_input(0, &input_tensor)?;
        self.session.compute()?;

        let mut eyes_tensor = Tensor::empty(TensorType::F32, self.model.outputs[0].shape.clone());
        let mut iris_tensor = Tensor::empty(TensorType::F32, self.model.outputs[1].shape.clone());

        self.session.get_output(0, &mut eyes_tensor)?;
        self.session.get_output(1, &mut iris_tensor)?;

        let eyes_data = eyes_tensor.as_f32();
        let iris_data = iris_tensor.as_f32();

        let num_eyes_landmarks = eyes_data.len() / 3;
        let mut eyes_contours = Vec::with_capacity(num_eyes_landmarks);
        for i in 0..num_eyes_landmarks {
            eyes_contours.push(Landmark::new(
                eyes_data[i * 3],
                eyes_data[i * 3 + 1],
                eyes_data[i * 3 + 2],
            ));
        }

        let num_iris_landmarks = iris_data.len() / 3;
        let mut iris = Vec::with_capacity(num_iris_landmarks);
        for i in 0..num_iris_landmarks {
            iris.push(Landmark::new(
                iris_data[i * 3],
                iris_data[i * 3 + 1],
                iris_data[i * 3 + 2],
            ));
        }

        Ok(IrisResult {
            eyes_contours,
            iris,
        })
    }
}
