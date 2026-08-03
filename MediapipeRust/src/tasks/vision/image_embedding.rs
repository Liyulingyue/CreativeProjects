use crate::backend::{Embedding, InferenceBackend, Model, Session, Tensor, TensorType, Error};

#[derive(Clone, Debug, Default)]
pub struct ImageEmbedderOptions {
    pub l2_normalize: bool,
    pub quantize: bool,
}

pub struct ImageEmbedderBuilder {
    options: ImageEmbedderOptions,
}

impl ImageEmbedderBuilder {
    pub fn new() -> Self {
        Self {
            options: ImageEmbedderOptions::default(),
        }
    }

    pub fn l2_normalize(mut self, normalize: bool) -> Self {
        self.options.l2_normalize = normalize;
        self
    }

    pub fn quantize(mut self, quantize: bool) -> Self {
        self.options.quantize = quantize;
        self
    }

    pub fn build_from_file<B: InferenceBackend>(self, backend: &B, path: &str) -> Result<ImageEmbedder, Error> {
        let data = std::fs::read(path)?;
        self.build_from_buffer(backend, data)
    }

    pub fn build_from_buffer<B: InferenceBackend>(self, backend: &B, buffer: Vec<u8>) -> Result<ImageEmbedder, Error> {
        let (model, session) = backend.load_model_and_session(&buffer)?;
        Ok(ImageEmbedder {
            model,
            session,
            options: self.options,
        })
    }
}

pub struct ImageEmbedder {
    model: Model,
    session: Session,
    options: ImageEmbedderOptions,
}

impl ImageEmbedder {
    pub fn embed(&mut self, image_data: &[u8], _width: u32, _height: u32) -> Result<Vec<Embedding>, Error> {
        let input_tensor = Tensor::new(
            self.model.inputs[0].tensor_type,
            self.model.inputs[0].shape.clone(),
            image_data.to_vec(),
        );
        self.session.set_input(0, &input_tensor)?;
        self.session.compute()?;

        let mut embedding_tensor = Tensor::empty(TensorType::F32, self.model.outputs[0].shape.clone());
        self.session.get_output(0, &mut embedding_tensor)?;

        let embedding_values = embedding_tensor.as_f32().to_vec();
        let embedding = if self.options.l2_normalize {
            self.l2_normalize(embedding_values)
        } else {
            embedding_values
        };

        Ok(vec![Embedding {
            values: embedding,
            label: None,
        }])
    }

    fn l2_normalize(&self, values: Vec<f32>) -> Vec<f32> {
        let sum: f32 = values.iter().map(|v| v * v).sum();
        let norm = sum.sqrt();
        if norm > 0.0 {
            values.iter().map(|v| v / norm).collect()
        } else {
            values
        }
    }
}
