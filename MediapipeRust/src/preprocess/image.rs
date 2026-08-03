use crate::backend::TensorType;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorSpaceType {
    RGB,
    RGBA,
    GRAYSCALE,
    BGR,
    YUV,
}

impl Default for ColorSpaceType {
    fn default() -> Self {
        Self::RGB
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ImageProperties {
    pub width: u32,
    pub height: u32,
    pub color_space: ColorSpaceType,
}

impl ImageProperties {
    pub fn new(width: u32, height: u32, color_space: ColorSpaceType) -> Self {
        Self { width, height, color_space }
    }

    pub fn channels(&self) -> u32 {
        match self.color_space {
            ColorSpaceType::RGB => 3,
            ColorSpaceType::RGBA => 4,
            ColorSpaceType::GRAYSCALE => 1,
            ColorSpaceType::BGR => 3,
            ColorSpaceType::YUV => 3,
        }
    }

    pub fn bytes_per_pixel(&self) -> u32 {
        self.channels()
    }
}

pub trait ImageToTensor {
    fn image_size(&self) -> (u32, u32);
    fn color_space(&self) -> ColorSpaceType;
    fn to_tensor_data(&self) -> Vec<u8>;
}

pub struct ImageToTensorOptions {
    pub normalize: bool,
    pub mean: [f32; 3],
    pub std: [f32; 3],
}

impl Default for ImageToTensorOptions {
    fn default() -> Self {
        Self {
            normalize: true,
            mean: [127.5, 127.5, 127.5],
            std: [127.5, 127.5, 127.5],
        }
    }
}

impl ImageToTensorOptions {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn normalize(mut self, normalize: bool) -> Self {
        self.normalize = normalize;
        self
    }

    pub fn mean(mut self, r: f32, g: f32, b: f32) -> Self {
        self.mean = [r, g, b];
        self
    }

    pub fn std(mut self, r: f32, g: f32, b: f32) -> Self {
        self.std = [r, g, b];
        self
    }
}

pub struct TensorConverter {
    options: ImageToTensorOptions,
    target_width: u32,
    target_height: u32,
    tensor_shape: Vec<usize>,
}

impl TensorConverter {
    pub fn new(target_width: u32, target_height: u32, options: ImageToTensorOptions) -> Self {
        let tensor_shape = vec![1, target_height as usize, target_width as usize, 4];
        Self {
            options,
            target_width,
            target_height,
            tensor_shape,
        }
    }

    pub fn convert(&self, image: &dyn ImageToTensor) -> Result<crate::backend::Tensor, crate::Error> {
        let (img_width, img_height) = image.image_size();
        let color_space = image.color_space();
        let image_data = image.to_tensor_data();

        let mut resized = self.resize(&image_data, img_width, img_height, color_space)?;

        if color_space != ColorSpaceType::RGBA {
            resized = self.convert_color_space(&resized, img_width, img_height, color_space)?;
        }

        if self.options.normalize {
            resized = self.normalize(resized);
        }

        Ok(crate::backend::Tensor::new(
            TensorType::F32,
            self.tensor_shape.clone(),
            resized,
        ))
    }

    fn resize(&self, data: &[u8], width: u32, height: u32, _color_space: ColorSpaceType) -> Result<Vec<u8>, crate::Error> {
        if width == self.target_width && height == self.target_height {
            return Ok(data.to_vec());
        }

        let channels = 4;
        let mut output = vec![0u8; (self.target_width * self.target_height * channels) as usize];

        let scale_x = width as f32 / self.target_width as f32;
        let scale_y = height as f32 / self.target_height as f32;

        for y in 0..self.target_height {
            for x in 0..self.target_width {
                let src_x = x as f32 * scale_x;
                let src_y = y as f32 * scale_y;

                let x0 = src_x as u32;
                let y0 = src_y as u32;
                let x1 = (x0 + 1).min(width - 1);
                let y1 = (y0 + 1).min(height - 1);

                let x_frac = src_x - x0 as f32;
                let y_frac = src_y - y0 as f32;

                let get_pixel = |px: u32, py: u32| -> [f32; 4] {
                    let idx = (py * width + px) * channels as u32;
                    if (idx as usize + 3) < data.len() {
                        [
                            data[idx as usize] as f32,
                            data[idx as usize + 1] as f32,
                            data[idx as usize + 2] as f32,
                            data[idx as usize + 3] as f32,
                        ]
                    } else {
                        [0.0, 0.0, 0.0, 255.0]
                    }
                };

                let p00 = get_pixel(x0, y0);
                let p01 = get_pixel(x0, y1);
                let p10 = get_pixel(x1, y0);
                let p11 = get_pixel(x1, y1);

                let dst_idx = (y * self.target_width + x) * channels as u32;

                for c in 0..4 {
                    let val = p00[c] * (1.0 - x_frac) * (1.0 - y_frac)
                            + p10[c] * x_frac * (1.0 - y_frac)
                            + p01[c] * (1.0 - x_frac) * y_frac
                            + p11[c] * x_frac * y_frac;
                    output[(dst_idx + c as u32) as usize] = val as u8;
                }
            }
        }

        Ok(output)
    }

    fn convert_color_space(&self, data: &[u8], width: u32, height: u32, _from: ColorSpaceType) -> Result<Vec<u8>, crate::Error> {
        let mut output = vec![0u8; (width * height * 4) as usize];
        for i in 0..(width * height) as usize {
            let src = i * 3;
            let dst = i * 4;
            if src + 2 < data.len() && dst + 3 < output.len() {
                output[dst] = data[src];
                output[dst + 1] = data[src + 1];
                output[dst + 2] = data[src + 2];
                output[dst + 3] = 255;
            }
        }
        Ok(output)
    }

    fn normalize(&self, data: Vec<u8>) -> Vec<u8> {
        let mut output = Vec::with_capacity(data.len());
        for chunk in data.chunks(4) {
            if chunk.len() == 4 {
                let r = ((chunk[0] as f32 - self.options.mean[0]) / self.options.std[0]) as u8;
                let g = ((chunk[1] as f32 - self.options.mean[1]) / self.options.std[1]) as u8;
                let b = ((chunk[2] as f32 - self.options.mean[2]) / self.options.std[2]) as u8;
                let a = chunk[3];
                output.extend_from_slice(&[r, g, b, a]);
            }
        }
        output
    }
}
