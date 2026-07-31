use crate::backend::{Embedding, InferenceBackend, Model, Session, Tensor, TensorType, Error};
use std::sync::Arc;

pub struct ImageEmbedderBuilder<B: InferenceBackend> {
    backend: B,
    options: ImageEmbedderOptions,
}

#[derive(Clone, Debug, Default)]
pub struct ImageEmbedderOptions {
    pub l2_normalize: bool,
    pub quantize: bool,
}

impl<B: InferenceBackend> ImageEmbedderBuilder<B> {
    pub fn new(backend: B) -> Self {
        Self {
            backend,
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

    pub fn build_from_file(self, path: &str) -> Result<ImageEmbedder<B>, Error> {
        let data = std::fs::read(path)?;
        self.build_from_buffer(data)
    }

    pub fn build_from_buffer(self, buffer: Vec<u8>) -> Result<ImageEmbedder<B>, Error> {
        let model = self.backend.load_model(&buffer)?;
        Ok(ImageEmbedder {
            backend: self.backend,
            model: Arc::new(model),
            options: self.options,
        })
    }
}

pub struct ImageEmbedder<B: InferenceBackend> {
    backend: B,
    model: Arc<Model>,
    options: ImageEmbedderOptions,
}

impl<B: InferenceBackend> ImageEmbedder<B> {
    pub fn new_session(&self) -> Result<ImageEmbedderSession<'_, B>, Error> {
        let session = self.backend.create_session(&*self.model)?;
        Ok(ImageEmbedderSession {
            embedder: self,
            session,
        })
    }

    pub fn embed(&self, image_data: &[u8], width: u32, height: u32) -> Result<Vec<Embedding>, Error> {
        let mut session = self.new_session()?;
        session.embed(image_data, width, height)
    }
}

pub struct ImageEmbedderSession<'a, B: InferenceBackend> {
    embedder: &'a ImageEmbedder<B>,
    session: Session,
}

impl<'a, B: InferenceBackend> ImageEmbedderSession<'a, B> {
    pub fn embed(&mut self, image_data: &[u8], width: u32, height: u32) -> Result<Vec<Embedding>, Error> {
        let input_tensor = Tensor::new(
            self.embedder.model.inputs[0].tensor_type,
            self.embedder.model.inputs[0].shape.clone(),
            image_data.to_vec(),
        );
        self.session.set_input(0, &input_tensor)?;
        self.session.compute()?;

        let mut embedding_tensor = Tensor::empty(TensorType::F32, self.embedder.model.outputs[0].shape.clone());
        self.session.get_output(0, &mut embedding_tensor)?;

        let embedding_values = embedding_tensor.as_f32().to_vec();
        let embedding = if self.embedder.options.l2_normalize {
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
