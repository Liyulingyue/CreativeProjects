use crate::backend::{Embedding, InferenceBackend, Model, Session, Tensor, TensorType, Error};
use std::sync::Arc;

pub struct TextEmbedderBuilder<B: InferenceBackend> {
    backend: B,
    options: TextEmbedderOptions,
}

#[derive(Clone, Debug, Default)]
pub struct TextEmbedderOptions {
    pub l2_normalize: bool,
    pub quantize: bool,
}

impl<B: InferenceBackend> TextEmbedderBuilder<B> {
    pub fn new(backend: B) -> Self {
        Self {
            backend,
            options: TextEmbedderOptions::default(),
        }
    }

    pub fn l2_normalize(mut self, normalize: bool) -> Self {
        self.options.l2_normalize = normalize;
        self
    }

    pub fn build_from_file(self, path: &str) -> Result<TextEmbedder<B>, Error> {
        let data = std::fs::read(path)?;
        self.build_from_buffer(data)
    }

    pub fn build_from_buffer(self, buffer: Vec<u8>) -> Result<TextEmbedder<B>, Error> {
        let model = self.backend.load_model(&buffer)?;
        Ok(TextEmbedder {
            backend: self.backend,
            model: Arc::new(model),
            options: self.options,
        })
    }
}

pub struct TextEmbedder<B: InferenceBackend> {
    backend: B,
    model: Arc<Model>,
    options: TextEmbedderOptions,
}

impl<B: InferenceBackend> TextEmbedder<B> {
    pub fn new_session(&self) -> Result<TextEmbedderSession<'_, B>, Error> {
        let session = self.backend.create_session(&*self.model)?;
        Ok(TextEmbedderSession {
            embedder: self,
            session,
        })
    }

    pub fn embed(&self, text: &str) -> Result<Embedding, Error> {
        let mut session = self.new_session()?;
        session.embed(text)
    }
}

pub struct TextEmbedderSession<'a, B: InferenceBackend> {
    embedder: &'a TextEmbedder<B>,
    session: Session,
}

impl<'a, B: InferenceBackend> TextEmbedderSession<'a, B> {
    pub fn embed(&mut self, text: &str) -> Result<Embedding, Error> {
        let input_tensor = self.text_to_tensor(text)?;
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

        Ok(Embedding {
            values: embedding,
            label: Some(text.to_string()),
        })
    }

    fn text_to_tensor(&self, text: &str) -> Result<Tensor, Error> {
        let input_shape = &self.embedder.model.inputs[0].shape;
        let bytes_per_element = self.embedder.model.inputs[0].tensor_type.byte_size();
        let total_size: usize = input_shape.iter().product::<usize>() * bytes_per_element;

        let mut data = vec![0u8; total_size];
        let text_bytes = text.as_bytes();

        let copy_len = text_bytes.len().min(total_size);
        data[..copy_len].copy_from_slice(&text_bytes[..copy_len]);

        Ok(Tensor::new(
            self.embedder.model.inputs[0].tensor_type,
            input_shape.clone(),
            data,
        ))
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
