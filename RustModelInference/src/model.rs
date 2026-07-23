use std::fs::File;

use memmap2::Mmap;

use crate::quant::dequantize_q4_k_weight;
use crate::traits::{ExecContext, Layer, ModelConfig};

const GGUF_MAGIC: &[u8; 4] = b"GGUF";
const GGUF_DEFAULT_ALIGNMENT: u64 = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(i32)]
pub enum GGMLType {
    F32 = 0,
    F16 = 1,
    Q4_0 = 2,
    Q4_1 = 3,
    Q5_0 = 6,
    Q5_1 = 7,
    Q8_0 = 8,
    Q8_1 = 9,
    Q2K = 10,
    Q3K = 11,
    Q4K = 12,
    Q5K = 13,
    Q6K = 14,
    Q8K = 15,
    I8 = 16,
    I16 = 17,
    I32 = 18,
}

impl GGMLType {
    pub fn from_i32(v: i32) -> Option<Self> {
        match v {
            0 => Some(Self::F32),
            1 => Some(Self::F16),
            2 => Some(Self::Q4_0),
            3 => Some(Self::Q4_1),
            6 => Some(Self::Q5_0),
            7 => Some(Self::Q5_1),
            8 => Some(Self::Q8_0),
            9 => Some(Self::Q8_1),
            10 => Some(Self::Q2K),
            11 => Some(Self::Q3K),
            12 => Some(Self::Q4K),
            13 => Some(Self::Q5K),
            14 => Some(Self::Q6K),
            15 => Some(Self::Q8K),
            16 => Some(Self::I8),
            17 => Some(Self::I16),
            18 => Some(Self::I32),
            _ => None,
        }
    }

    pub fn type_traits(self) -> (usize, usize) {
        match self {
            Self::F32 => (1, 4),
            Self::F16 => (1, 2),
            Self::Q4_0 => (32, 18),
            Self::Q4_1 => (32, 20),
            Self::Q5_0 => (32, 22),
            Self::Q5_1 => (32, 24),
            Self::Q8_0 => (32, 34),
            Self::Q8_1 => (32, 36),
            Self::Q2K => (256, 256),
            Self::Q3K => (256, 256),
            Self::Q4K => (256, 144),
            Self::Q5K => (256, 176),
            Self::Q6K => (256, 210),
            Self::Q8K => (256, 292),
            Self::I8 => (1, 1),
            Self::I16 => (1, 2),
            Self::I32 => (1, 4),
        }
    }

    pub fn nbytes(self, n_elements: usize) -> usize {
        let (block_size, type_size) = self.type_traits();
        let n_blocks = (n_elements + block_size - 1) / block_size;
        n_blocks * type_size
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(i32)]
pub enum MetaValueType {
    Uint8 = 0,
    Int8 = 1,
    Uint16 = 2,
    Int16 = 3,
    Uint32 = 4,
    Int32 = 5,
    Float32 = 6,
    Bool = 7,
    String = 8,
    Array = 9,
    Uint64 = 10,
    Int64 = 11,
    Float64 = 12,
}

impl MetaValueType {
    pub fn from_i32(v: i32) -> Option<Self> {
        match v {
            0 => Some(Self::Uint8),
            1 => Some(Self::Int8),
            2 => Some(Self::Uint16),
            3 => Some(Self::Int16),
            4 => Some(Self::Uint32),
            5 => Some(Self::Int32),
            6 => Some(Self::Float32),
            7 => Some(Self::Bool),
            8 => Some(Self::String),
            9 => Some(Self::Array),
            10 => Some(Self::Uint64),
            11 => Some(Self::Int64),
            12 => Some(Self::Float64),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub enum MetaValue {
    Uint8(u8),
    Int8(i8),
    Uint16(u16),
    Int16(i16),
    Uint32(u32),
    Int32(i32),
    Float32(f32),
    Bool(bool),
    String(String),
    Uint64(u64),
    Int64(i64),
    Float64(f64),
    Array(MetaValueType, Vec<MetaValue>),
}

impl MetaValue {
    pub fn to_u64(&self) -> Option<u64> {
        match self {
            Self::Uint8(v) => Some(*v as u64),
            Self::Int8(v) => Some(*v as u64),
            Self::Uint16(v) => Some(*v as u64),
            Self::Int16(v) => Some(*v as u64),
            Self::Uint32(v) => Some(*v as u64),
            Self::Int32(v) => Some(*v as u64),
            Self::Uint64(v) => Some(*v),
            Self::Int64(v) => Some(*v as u64),
            Self::Float32(v) => Some(*v as u64),
            Self::Float64(v) => Some(*v as u64),
            _ => None,
        }
    }

    pub fn to_f64(&self) -> Option<f64> {
        match self {
            Self::Float32(v) => Some(*v as f64),
            Self::Float64(v) => Some(*v),
            Self::Uint32(v) => Some(*v as f64),
            Self::Int32(v) => Some(*v as f64),
            Self::Uint64(v) => Some(*v as f64),
            Self::Int64(v) => Some(*v as f64),
            _ => None,
        }
    }

    pub fn to_string_val(&self) -> Option<&str> {
        match self {
            Self::String(s) => Some(s),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct TensorInfo {
    pub name: String,
    pub dims: Vec<u64>,
    pub ggml_type: GGMLType,
    pub offset: u64,
}

impl TensorInfo {
    pub fn n_elements(&self) -> usize {
        self.dims.iter().product::<u64>() as usize
    }

    pub fn nbytes(&self) -> usize {
        self.ggml_type.nbytes(self.n_elements())
    }
}

struct ByteReader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> ByteReader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    fn remaining(&self) -> usize {
        self.data.len().saturating_sub(self.pos)
    }

    fn read_u8(&mut self) -> Result<u8, String> {
        if self.remaining() < 1 {
            return Err("EOF reading u8".into());
        }
        let v = self.data[self.pos];
        self.pos += 1;
        Ok(v)
    }

    fn read_i8(&mut self) -> Result<i8, String> {
        Ok(self.read_u8()? as i8)
    }

    fn read_u16(&mut self) -> Result<u16, String> {
        if self.remaining() < 2 {
            return Err("EOF reading u16".into());
        }
        let v = u16::from_le_bytes([self.data[self.pos], self.data[self.pos + 1]]);
        self.pos += 2;
        Ok(v)
    }

    fn read_i16(&mut self) -> Result<i16, String> {
        Ok(self.read_u16()? as i16)
    }

    fn read_u32(&mut self) -> Result<u32, String> {
        if self.remaining() < 4 {
            return Err("EOF reading u32".into());
        }
        let v = u32::from_le_bytes([
            self.data[self.pos],
            self.data[self.pos + 1],
            self.data[self.pos + 2],
            self.data[self.pos + 3],
        ]);
        self.pos += 4;
        Ok(v)
    }

    fn read_i32(&mut self) -> Result<i32, String> {
        Ok(self.read_u32()? as i32)
    }

    fn read_u64(&mut self) -> Result<u64, String> {
        if self.remaining() < 8 {
            return Err("EOF reading u64".into());
        }
        let mut bytes = [0u8; 8];
        bytes.copy_from_slice(&self.data[self.pos..self.pos + 8]);
        self.pos += 8;
        Ok(u64::from_le_bytes(bytes))
    }

    fn read_i64(&mut self) -> Result<i64, String> {
        Ok(self.read_u64()? as i64)
    }

    fn read_f32(&mut self) -> Result<f32, String> {
        Ok(f32::from_bits(self.read_u32()?))
    }

    fn read_f64(&mut self) -> Result<f64, String> {
        Ok(f64::from_bits(self.read_u64()?))
    }

    fn read_string(&mut self) -> Result<String, String> {
        let len = self.read_u64()? as usize;
        if self.remaining() < len {
            return Err(format!("EOF reading string of len {}", len));
        }
        let s = String::from_utf8_lossy(&self.data[self.pos..self.pos + len]).into_owned();
        self.pos += len;
        Ok(s)
    }

    fn read_meta_value(&mut self, vtype: MetaValueType) -> Result<MetaValue, String> {
        match vtype {
            MetaValueType::Uint8 => Ok(MetaValue::Uint8(self.read_u8()?)),
            MetaValueType::Int8 => Ok(MetaValue::Int8(self.read_i8()?)),
            MetaValueType::Uint16 => Ok(MetaValue::Uint16(self.read_u16()?)),
            MetaValueType::Int16 => Ok(MetaValue::Int16(self.read_i16()?)),
            MetaValueType::Uint32 => Ok(MetaValue::Uint32(self.read_u32()?)),
            MetaValueType::Int32 => Ok(MetaValue::Int32(self.read_i32()?)),
            MetaValueType::Float32 => Ok(MetaValue::Float32(self.read_f32()?)),
            MetaValueType::Bool => Ok(MetaValue::Bool(self.read_u8()? != 0)),
            MetaValueType::String => Ok(MetaValue::String(self.read_string()?)),
            MetaValueType::Uint64 => Ok(MetaValue::Uint64(self.read_u64()?)),
            MetaValueType::Int64 => Ok(MetaValue::Int64(self.read_i64()?)),
            MetaValueType::Float64 => Ok(MetaValue::Float64(self.read_f64()?)),
            MetaValueType::Array => {
                let elem_type_i32 = self.read_i32()?;
                let elem_type = MetaValueType::from_i32(elem_type_i32)
                    .ok_or_else(|| format!("Unknown meta value type: {}", elem_type_i32))?;
                let n = self.read_u64()? as usize;
                let mut vals = Vec::with_capacity(n);
                for _ in 0..n {
                    vals.push(self.read_meta_value(elem_type)?);
                }
                Ok(MetaValue::Array(elem_type, vals))
            }
        }
    }

    fn pos(&self) -> usize {
        self.pos
    }
}

pub struct GGUFLoader {
    mmap: Mmap,
    pub version: u32,
    pub alignment: u64,
    data_offset: usize,
    metadata: Vec<(String, MetaValue)>,
    tensors: Vec<TensorInfo>,
}

impl std::fmt::Debug for GGUFLoader {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GGUFLoader")
            .field("version", &self.version)
            .field("alignment", &self.alignment)
            .field("data_offset", &self.data_offset)
            .field("n_metadata", &self.metadata.len())
            .field("n_tensors", &self.tensors.len())
            .finish()
    }
}

impl GGUFLoader {
    pub fn from_file(path: &str) -> Result<Self, String> {
        let file = File::open(path).map_err(|e| format!("Failed to open GGUF file: {}", e))?;
        let mmap = unsafe { Mmap::map(&file) }.map_err(|e| format!("Failed to mmap: {}", e))?;
        Self::from_mmap(mmap)
    }

    pub fn from_mmap(mmap: Mmap) -> Result<Self, String> {
        let mut reader = ByteReader::new(&mmap);

        let magic = reader.read_u32()?;
        if &magic.to_le_bytes() != GGUF_MAGIC {
            return Err(format!(
                "Invalid GGUF magic: {:?} (expected {:?})",
                &magic.to_le_bytes(),
                GGUF_MAGIC
            ));
        }

        let version = reader.read_u32()?;
        if version < 2 || version > 3 {
            return Err(format!("Unsupported GGUF version: {}", version));
        }

        let n_tensors = reader.read_u64()?;
        let n_kv = reader.read_u64()?;

        let mut metadata = Vec::with_capacity(n_kv as usize);
        for _ in 0..n_kv {
            let key = reader.read_string()?;
            let vtype_i32 = reader.read_i32()?;
            let vtype = MetaValueType::from_i32(vtype_i32)
                .ok_or_else(|| format!("Unknown meta value type: {}", vtype_i32))?;
            let value = reader.read_meta_value(vtype)?;
            metadata.push((key, value));
        }

        let mut tensors = Vec::with_capacity(n_tensors as usize);
        for _ in 0..n_tensors {
            let name = reader.read_string()?;
            let n_dims = reader.read_u32()?;
            let mut dims = Vec::with_capacity(n_dims as usize);
            for _ in 0..n_dims {
                dims.push(reader.read_u64()?);
            }
            let type_i32 = reader.read_i32()?;
            let ggml_type = GGMLType::from_i32(type_i32)
                .ok_or_else(|| format!("Unknown GGML type: {}", type_i32))?;
            let offset = reader.read_u64()?;
            tensors.push(TensorInfo {
                name,
                dims,
                ggml_type,
                offset,
            });
        }

        let alignment = metadata
            .iter()
            .find(|(k, _)| k == "general.alignment")
            .and_then(|(_, v)| v.to_u64())
            .unwrap_or(GGUF_DEFAULT_ALIGNMENT);

        let data_offset = reader.pos();
        let padded_data_offset =
            ((data_offset as u64 + alignment - 1) / alignment * alignment) as usize;

        Ok(Self {
            mmap,
            version,
            alignment,
            data_offset: padded_data_offset,
            metadata,
            tensors,
        })
    }

    pub fn n_tensors(&self) -> usize {
        self.tensors.len()
    }

    pub fn n_kv(&self) -> usize {
        self.metadata.len()
    }

    pub fn data_offset(&self) -> usize {
        self.data_offset
    }

    pub fn tensors(&self) -> &[TensorInfo] {
        &self.tensors
    }

    pub fn metadata(&self, key: &str) -> Option<&MetaValue> {
        for (k, v) in &self.metadata {
            if k == key {
                return Some(v);
            }
        }
        None
    }

    pub fn metadata_keys(&self) -> impl Iterator<Item = &str> {
        self.metadata.iter().map(|(k, _)| k.as_str())
    }

    pub fn tensor_info(&self, name: &str) -> Option<&TensorInfo> {
        for t in &self.tensors {
            if t.name == name {
                return Some(t);
            }
        }
        None
    }

    pub fn tensor_slice(&self, name: &str) -> Option<&[u8]> {
        let tensor = self.tensor_info(name)?;
        let abs_offset = self.data_offset + tensor.offset as usize;
        let nbytes = tensor.nbytes();
        if abs_offset + nbytes <= self.mmap.len() {
            Some(&self.mmap[abs_offset..abs_offset + nbytes])
        } else {
            None
        }
    }

    pub fn model_config(&self) -> Result<ModelConfig, String> {
        let arch = self
            .metadata("general.architecture")
            .and_then(|v| v.to_string_val())
            .unwrap_or_default();

        let prefix = match &arch as &str {
            "qwen2" | "qwen3" => arch,
            _ => return Err(format!("Unsupported architecture: {}", arch)),
        };

        let get_u64 = |key: &str| -> Result<u64, String> {
            self.metadata(key)
                .and_then(|v| v.to_u64())
                .ok_or_else(|| format!("Missing metadata: {}", key))
        };

        let get_f64 = |key: &str| -> Result<f64, String> {
            self.metadata(key)
                .and_then(|v| v.to_f64())
                .ok_or_else(|| format!("Missing metadata: {}", key))
        };

        let n_embd = get_u64(&format!("{}.embedding_length", prefix))? as usize;
        let n_head = get_u64(&format!("{}.attention.head_count", prefix))? as usize;

        Ok(ModelConfig {
            n_embd,
            n_layer: get_u64(&format!("{}.block_count", prefix))? as usize,
            n_head,
            n_head_kv: get_u64(&format!("{}.attention.head_count_kv", prefix))? as usize,
            n_embd_head: n_embd / n_head,
            n_ff: get_u64(&format!("{}.feed_forward_length", prefix))? as usize,
            n_ctx: get_u64(&format!("{}.context_length", prefix))? as usize,
            vocab_size: get_u64(&format!("{}.attention.head_count", prefix))? as usize,
            rope_freq_base: get_f64(&format!("{}.rope.freq_base", prefix))? as f32,
            norm_eps: get_f64(&format!("{}.attention.layer_norm_rms_epsilon", prefix))? as f32,
        })
    }
}

pub struct QuantizedLinear<'a> {
    weight: &'a [u8],
    #[allow(dead_code)]
    bias: Option<&'a [u8]>,
    in_features: usize,
    out_features: usize,
    layer_name: &'a str,
}

impl<'a> QuantizedLinear<'a> {
    pub fn from_weight_slice(
        weight: &'a [u8],
        bias: Option<&'a [u8]>,
        in_features: usize,
        out_features: usize,
        name: &'a str,
    ) -> Self {
        Self {
            weight,
            bias,
            in_features,
            out_features,
            layer_name: name,
        }
    }

    pub fn from_gguf(
        loader: &'a GGUFLoader,
        weight_name: &str,
        bias_name: Option<&str>,
        in_features: usize,
        out_features: usize,
        name: &'a str,
    ) -> Option<Self> {
        let weight = loader.tensor_slice(weight_name)?;
        let bias = bias_name.and_then(|n| loader.tensor_slice(n));
        Some(Self {
            weight,
            bias,
            in_features,
            out_features,
            layer_name: name,
        })
    }

    pub fn weight_ptr(&self) -> usize {
        self.weight.as_ptr() as usize
    }

    pub fn weight_len(&self) -> usize {
        self.weight.len()
    }

    pub fn forward_dequant(&self, input: &[f32], output: &mut [f32], scratch: &mut [f32]) {
        let n_elements = self.out_features * self.in_features;
        let dequant_len = n_elements.min(scratch.len());
        dequantize_q4_k_weight(
            self.weight,
            self.out_features,
            self.in_features,
            &mut scratch[..dequant_len],
        );

        let dequant = &scratch[..dequant_len];

        for i in 0..self.out_features {
            let row_offset = i * self.in_features;
            let mut sum = 0.0f32;
            for j in 0..self.in_features {
                sum += dequant[row_offset + j] * input[j];
            }
            output[i] = sum;
        }
    }
}

impl<'a> Layer for QuantizedLinear<'a> {
    fn forward(&self, input: &[f32], output: &mut [f32], ctx: &mut ExecContext) {
        self.forward_dequant(input, output, ctx.scratch);
    }

    fn input_dim(&self) -> usize {
        self.in_features
    }

    fn output_dim(&self) -> usize {
        self.out_features
    }

    fn name(&self) -> &str {
        self.layer_name
    }
}

pub struct ModelGraph<'a> {
    pub config: ModelConfig,
    pub layers: Vec<Box<dyn Layer + 'a>>,
}

impl<'a> ModelGraph<'a> {
    pub fn new(config: ModelConfig) -> Self {
        Self {
            config,
            layers: Vec::new(),
        }
    }

    pub fn add_layer<L: Layer + 'a>(&mut self, layer: L) {
        self.layers.push(Box::new(layer));
    }

    pub fn forward_all(
        &self,
        input: &[f32],
        output: &mut [f32],
        scratch: &mut [f32],
        ctx: &mut ExecContext,
    ) {
        if self.layers.is_empty() {
            return;
        }

        let dim = self.layers[0].output_dim().max(self.layers[0].input_dim());
        let (buf_a, buf_b) = scratch.split_at_mut(dim);

        buf_a[..input.len()].copy_from_slice(input);

        for (i, layer) in self.layers.iter().enumerate() {
            ctx.layer_idx = i as u32;
            if i % 2 == 0 {
                layer.forward(buf_a, buf_b, ctx);
            } else {
                layer.forward(buf_b, buf_a, ctx);
            }
        }

        let last_idx = self.layers.len() - 1;
        let src = if last_idx % 2 == 0 { buf_b } else { buf_a };
        let out_len = output.len().min(src.len());
        output[..out_len].copy_from_slice(&src[..out_len]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn push_u8(buf: &mut Vec<u8>, v: u8) { buf.push(v); }
    fn push_u16(buf: &mut Vec<u8>, v: u16) { buf.extend_from_slice(&v.to_le_bytes()); }
    fn push_i16(buf: &mut Vec<u8>, v: i16) { buf.extend_from_slice(&v.to_le_bytes()); }
    fn push_u32(buf: &mut Vec<u8>, v: u32) { buf.extend_from_slice(&v.to_le_bytes()); }
    fn push_i32(buf: &mut Vec<u8>, v: i32) { buf.extend_from_slice(&v.to_le_bytes()); }
    fn push_u64(buf: &mut Vec<u8>, v: u64) { buf.extend_from_slice(&v.to_le_bytes()); }
    fn push_i64(buf: &mut Vec<u8>, v: i64) { buf.extend_from_slice(&v.to_le_bytes()); }
    fn push_f32(buf: &mut Vec<u8>, v: f32) { buf.extend_from_slice(&v.to_le_bytes()); }
    fn push_f64(buf: &mut Vec<u8>, v: f64) { buf.extend_from_slice(&v.to_le_bytes()); }

    fn push_str(buf: &mut Vec<u8>, s: &str) {
        push_u64(buf, s.len() as u64);
        buf.extend_from_slice(s.as_bytes());
    }

    fn push_kv(buf: &mut Vec<u8>, key: &str, vtype: MetaValueType, write_val: impl FnOnce(&mut Vec<u8>)) {
        push_str(buf, key);
        push_i32(buf, vtype as i32);
        write_val(buf);
    }

    fn build_minimal_gguf() -> Vec<u8> {
        let mut b = Vec::new();
        push_u32(&mut b, u32::from_le_bytes(*b"GGUF"));
        push_u32(&mut b, 3);
        push_u64(&mut b, 2);
        push_u64(&mut b, 5);

        push_kv(&mut b, "general.architecture", MetaValueType::String, |b| push_str(b, "qwen2"));
        push_kv(&mut b, "qwen2.embedding_length", MetaValueType::Uint32, |b| push_u32(b, 1024));
        push_kv(&mut b, "qwen2.block_count", MetaValueType::Uint32, |b| push_u32(b, 24));
        push_kv(&mut b, "qwen2.attention.head_count", MetaValueType::Uint32, |b| push_u32(b, 16));
        push_kv(&mut b, "general.alignment", MetaValueType::Uint64, |b| push_u64(b, 32));

        push_str(&mut b, "token_embd.weight");
        push_u32(&mut b, 2);
        push_u64(&mut b, 1024);
        push_u64(&mut b, 151936);
        push_i32(&mut b, GGMLType::Q4K as i32);
        push_u64(&mut b, 0);

        push_str(&mut b, "blk.0.attn_q.weight");
        push_u32(&mut b, 2);
        push_u64(&mut b, 1024);
        push_u64(&mut b, 1024);
        push_i32(&mut b, GGMLType::Q4K as i32);
        let embd_nbytes = GGMLType::Q4K.nbytes(1024 * 151936);
        let padded = ((embd_nbytes as u64 + 31) / 32 * 32) as u64;
        push_u64(&mut b, padded);

        while b.len() % 32 != 0 { b.push(0); }
        let data_start = b.len();
        let total_data = padded as usize + GGMLType::Q4K.nbytes(1024 * 1024);
        b.resize(data_start + total_data, 0xAB);
        b
    }

    fn build_all_meta_types_gguf() -> Vec<u8> {
        let mut b = Vec::new();
        push_u32(&mut b, u32::from_le_bytes(*b"GGUF"));
        push_u32(&mut b, 3);
        push_u64(&mut b, 0);
        push_u64(&mut b, 13);

        push_kv(&mut b, "test.uint8", MetaValueType::Uint8, |b| push_u8(b, 42));
        push_kv(&mut b, "test.int8", MetaValueType::Int8, |b| push_u8(b, (-1i8) as u8));
        push_kv(&mut b, "test.uint16", MetaValueType::Uint16, |b| push_u16(b, 1000));
        push_kv(&mut b, "test.int16", MetaValueType::Int16, |b| push_i16(b, -100));
        push_kv(&mut b, "test.uint32", MetaValueType::Uint32, |b| push_u32(b, 1024));
        push_kv(&mut b, "test.int32", MetaValueType::Int32, |b| push_i32(b, -24));
        push_kv(&mut b, "test.float32", MetaValueType::Float32, |b| push_f32(b, 3.14));
        push_kv(&mut b, "test.bool", MetaValueType::Bool, |b| push_u8(b, 1));
        push_kv(&mut b, "test.string", MetaValueType::String, |b| push_str(b, "hello"));
        push_kv(&mut b, "test.uint64", MetaValueType::Uint64, |b| push_u64(b, 999999));
        push_kv(&mut b, "test.int64", MetaValueType::Int64, |b| push_i64(b, -123456));
        push_kv(&mut b, "test.float64", MetaValueType::Float64, |b| push_f64(b, 2.71828));
        push_kv(&mut b, "test.array", MetaValueType::Array, |b| {
            push_i32(b, MetaValueType::Uint32 as i32);
            push_u64(b, 3);
            push_u32(b, 10);
            push_u32(b, 20);
            push_u32(b, 30);
        });

        while b.len() % 32 != 0 { b.push(0); }
        b
    }

    static COUNTER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

    fn parse_temp(data: &[u8]) -> Result<GGUFLoader, String> {
        let dir = std::env::temp_dir().join("rust_model_inference_test");
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let id = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let path = dir.join(format!("test_{}_{}.gguf", std::process::id(), id));
        std::fs::write(&path, data).map_err(|e| e.to_string())?;
        GGUFLoader::from_file(path.to_str().unwrap())
    }

    #[test]
    fn test_gguf_parse_minimal() {
        let data = build_minimal_gguf();
        let loader = parse_temp(&data).expect("parse minimal GGUF");

        assert_eq!(loader.version, 3);
        assert_eq!(loader.alignment, 32);
        assert_eq!(loader.n_tensors(), 2);
        assert_eq!(loader.n_kv(), 5);

        let arch = loader.metadata("general.architecture").and_then(|v| v.to_string_val()).unwrap();
        assert_eq!(arch, "qwen2");
        assert_eq!(loader.metadata("qwen2.embedding_length").and_then(|v| v.to_u64()), Some(1024));
        assert_eq!(loader.metadata("qwen2.block_count").and_then(|v| v.to_u64()), Some(24));

        let ti0 = loader.tensor_info("token_embd.weight").expect("tensor 0");
        assert_eq!(ti0.dims, vec![1024, 151936]);
        assert_eq!(ti0.ggml_type, GGMLType::Q4K);
        assert_eq!(ti0.offset, 0);

        let ti1 = loader.tensor_info("blk.0.attn_q.weight").expect("tensor 1");
        assert_eq!(ti1.dims, vec![1024, 1024]);

        let s0 = loader.tensor_slice("token_embd.weight").expect("slice 0");
        assert_eq!(s0.len(), ti0.nbytes());
        let s1 = loader.tensor_slice("blk.0.attn_q.weight").expect("slice 1");
        assert_eq!(s1.len(), ti1.nbytes());
    }

    #[test]
    fn test_gguf_all_meta_types() {
        let data = build_all_meta_types_gguf();
        let loader = parse_temp(&data).expect("parse all meta types");

        assert_eq!(loader.metadata("test.uint8").and_then(|v| v.to_u64()), Some(42));
        assert_eq!(loader.metadata("test.int8").and_then(|v| v.to_u64()), Some((-1i8) as u64));
        assert_eq!(loader.metadata("test.uint16").and_then(|v| v.to_u64()), Some(1000));
        assert_eq!(loader.metadata("test.int16").and_then(|v| v.to_u64()), Some((-100i16) as u64));
        assert_eq!(loader.metadata("test.uint32").and_then(|v| v.to_u64()), Some(1024));
        assert_eq!(loader.metadata("test.int32").and_then(|v| v.to_u64()), Some((-24i32) as u64));

        let f32v = loader.metadata("test.float32").and_then(|v| v.to_f64()).unwrap();
        assert!((f32v - 3.14).abs() < 0.01);

        match loader.metadata("test.bool") {
            Some(MetaValue::Bool(true)) => {},
            other => panic!("expected Bool(true), got {:?}", other),
        }

        assert_eq!(loader.metadata("test.string").and_then(|v| v.to_string_val()), Some("hello"));
        assert_eq!(loader.metadata("test.uint64").and_then(|v| v.to_u64()), Some(999999));
        assert_eq!(loader.metadata("test.int64").and_then(|v| v.to_u64()), Some((-123456i64) as u64));

        let f64v = loader.metadata("test.float64").and_then(|v| v.to_f64()).unwrap();
        assert!((f64v - 2.71828).abs() < 0.001);

        match loader.metadata("test.array") {
            Some(MetaValue::Array(et, vals)) => {
                assert_eq!(*et, MetaValueType::Uint32);
                assert_eq!(vals.len(), 3);
                assert_eq!(vals[0].to_u64(), Some(10));
                assert_eq!(vals[1].to_u64(), Some(20));
                assert_eq!(vals[2].to_u64(), Some(30));
            },
            other => panic!("expected Array, got {:?}", other),
        }
    }

    #[test]
    fn test_ggml_type_nbytes() {
        assert_eq!(GGMLType::F32.nbytes(256), 1024);
        assert_eq!(GGMLType::F16.nbytes(256), 512);
        assert_eq!(GGMLType::Q4K.nbytes(256), 144);
        assert_eq!(GGMLType::Q4K.nbytes(512), 288);
        assert_eq!(GGMLType::Q4_0.nbytes(32), 18);
        assert_eq!(GGMLType::Q8_0.nbytes(32), 34);
    }

    #[test]
    fn test_gguf_invalid_magic() {
        let mut b = Vec::new();
        push_u32(&mut b, u32::from_le_bytes(*b"GGML"));
        push_u32(&mut b, 3);
        push_u64(&mut b, 0);
        push_u64(&mut b, 0);
        let err = parse_temp(&b).unwrap_err();
        assert!(err.contains("Invalid GGUF magic"), "got: {}", err);
    }

    #[test]
    fn test_gguf_bad_version() {
        let mut b = Vec::new();
        push_u32(&mut b, u32::from_le_bytes(*b"GGUF"));
        push_u32(&mut b, 1);
        push_u64(&mut b, 0);
        push_u64(&mut b, 0);
        let err = parse_temp(&b).unwrap_err();
        assert!(err.contains("Unsupported GGUF version"), "got: {}", err);
    }
}
