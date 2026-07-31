use crate::backend::{Class, InferenceBackend, Model, Session, Tensor, TensorType, Error};
use std::sync::Arc;

pub struct TextClassifierBuilder<B: InferenceBackend> {
    backend: B,
    options: TextClassifierOptions,
}

#[derive(Clone, Debug, Default)]
pub struct TextClassifierOptions {
    pub label_allow_list: Vec<String>,
    pub label_deny_list: Vec<String>,
    pub max_results: i32,
}

impl<B: InferenceBackend> TextClassifierBuilder<B> {
    pub fn new(backend: B) -> Self {
        Self {
            backend,
            options: TextClassifierOptions::default(),
        }
    }

    pub fn max_results(mut self, max: i32) -> Self {
        self.options.max_results = max;
        self
    }

    pub fn build_from_file(self, path: &str) -> Result<TextClassifier<B>, Error> {
        let data = std::fs::read(path)?;
        self.build_from_buffer(data)
    }

    pub fn build_from_buffer(self, buffer: Vec<u8>) -> Result<TextClassifier<B>, Error> {
        let model = self.backend.load_model(&buffer)?;
        Ok(TextClassifier {
            backend: self.backend,
            model: Arc::new(model),
            options: self.options,
        })
    }
}

pub struct TextClassifier<B: InferenceBackend> {
    backend: B,
    model: Arc<Model>,
    options: TextClassifierOptions,
}

impl<B: InferenceBackend> TextClassifier<B> {
    pub fn new_session(&self) -> Result<TextClassifierSession<'_, B>, Error> {
        let session = self.backend.create_session(&*self.model)?;
        Ok(TextClassifierSession {
            classifier: self,
            session,
        })
    }

    pub fn classify(&self, text: &str) -> Result<Vec<Class>, Error> {
        let mut session = self.new_session()?;
        session.classify(text)
    }
}

pub struct TextClassifierSession<'a, B: InferenceBackend> {
    classifier: &'a TextClassifier<B>,
    session: Session,
}

impl<'a, B: InferenceBackend> TextClassifierSession<'a, B> {
    pub fn classify(&mut self, text: &str) -> Result<Vec<Class>, Error> {
        let input_tensor = self.text_to_tensor(text)?;
        self.session.set_input(0, &input_tensor)?;
        self.session.compute()?;

        let mut output_tensor = Tensor::empty(TensorType::F32, self.classifier.model.outputs[0].shape.clone());
        self.session.get_output(0, &mut output_tensor)?;

        let scores = output_tensor.as_f32();
        let mut classes: Vec<Class> = scores
            .iter()
            .enumerate()
            .map(|(i, &s)| Class::new(i as i32, s, format!("class_{}", i)))
            .collect();

        classes.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());
        classes.truncate(self.classifier.options.max_results as usize);

        Ok(classes)
    }

    fn text_to_tensor(&self, text: &str) -> Result<Tensor, Error> {
        let input_shape = &self.classifier.model.inputs[0].shape;
        let bytes_per_element = self.classifier.model.inputs[0].tensor_type.byte_size();
        let total_size: usize = input_shape.iter().product::<usize>() * bytes_per_element;

        let mut data = vec![0u8; total_size];
        let text_bytes = text.as_bytes();

        let copy_len = text_bytes.len().min(total_size);
        data[..copy_len].copy_from_slice(&text_bytes[..copy_len]);

        Ok(Tensor::new(
            self.classifier.model.inputs[0].tensor_type,
            input_shape.clone(),
            data,
        ))
    }
}
