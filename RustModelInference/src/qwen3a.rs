use crate::model::{GGMLType, MetaValue, TensorSource};
use rustfft::{num_complex::Complex, FftPlanner};

const SAMPLE_RATE: usize = 16_000;
const FFT_SIZE: usize = 400;
const HOP: usize = 160;
const MEL_BINS: usize = 128;
const WINDOW_FRAMES: usize = 800;
const CHUNK_FRAMES: usize = 100;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AsrAudioError {
    Unsupported(String),
    Invalid(String),
}

fn wav_u16(bytes: &[u8], offset: usize) -> Result<u16, AsrAudioError> {
    let end = offset
        .checked_add(2)
        .ok_or_else(|| AsrAudioError::Invalid("WAV offset overflow".into()))?;
    let value = bytes
        .get(offset..end)
        .ok_or_else(|| AsrAudioError::Invalid("truncated WAV field".into()))?;
    Ok(u16::from_le_bytes(value.try_into().unwrap()))
}

fn wav_u32(bytes: &[u8], offset: usize) -> Result<u32, AsrAudioError> {
    let end = offset
        .checked_add(4)
        .ok_or_else(|| AsrAudioError::Invalid("WAV offset overflow".into()))?;
    let value = bytes
        .get(offset..end)
        .ok_or_else(|| AsrAudioError::Invalid("truncated WAV field".into()))?;
    Ok(u32::from_le_bytes(value.try_into().unwrap()))
}

pub fn decode_pcm16_wav(bytes: &[u8]) -> Result<Vec<f32>, AsrAudioError> {
    if bytes.get(0..4) != Some(b"RIFF") || bytes.get(8..12) != Some(b"WAVE") {
        return Err(AsrAudioError::Unsupported("expected RIFF/WAVE".into()));
    }
    let riff_end = 8usize
        .checked_add(wav_u32(bytes, 4)? as usize)
        .ok_or_else(|| AsrAudioError::Invalid("RIFF size overflow".into()))?;
    if riff_end > bytes.len() {
        return Err(AsrAudioError::Invalid("truncated RIFF".into()));
    }

    let mut format = None;
    let mut pcm = None;
    let mut offset = 12usize;
    while offset < riff_end {
        let id_end = offset
            .checked_add(4)
            .ok_or_else(|| AsrAudioError::Invalid("chunk offset overflow".into()))?;
        let header_end = offset
            .checked_add(8)
            .ok_or_else(|| AsrAudioError::Invalid("chunk header overflow".into()))?;
        let id = bytes
            .get(offset..id_end)
            .ok_or_else(|| AsrAudioError::Invalid("truncated chunk header".into()))?;
        let len = wav_u32(bytes, id_end)? as usize;
        let data_end = header_end
            .checked_add(len)
            .ok_or_else(|| AsrAudioError::Invalid("chunk size overflow".into()))?;
        let padded_end = data_end
            .checked_add(len & 1)
            .ok_or_else(|| AsrAudioError::Invalid("chunk padding overflow".into()))?;
        if padded_end > riff_end {
            return Err(AsrAudioError::Invalid(
                "truncated chunk data or padding".into(),
            ));
        }
        let chunk = bytes
            .get(header_end..data_end)
            .ok_or_else(|| AsrAudioError::Invalid("truncated chunk data".into()))?;
        match id {
            b"fmt " => {
                if format.replace(chunk).is_some() {
                    return Err(AsrAudioError::Invalid("duplicate fmt chunk".into()));
                }
            }
            b"data" => {
                if pcm.replace(chunk).is_some() {
                    return Err(AsrAudioError::Invalid("duplicate data chunk".into()));
                }
            }
            _ => {}
        }
        offset = padded_end;
    }

    let format = format.ok_or_else(|| AsrAudioError::Invalid("missing fmt chunk".into()))?;
    let expected = [
        (wav_u16(format, 0)? as u32, 1, "PCM format"),
        (wav_u16(format, 2)? as u32, 1, "mono audio"),
        (wav_u32(format, 4)?, 16_000, "16000 Hz sample rate"),
        (wav_u32(format, 8)?, 32_000, "32000 byte rate"),
        (wav_u16(format, 12)? as u32, 2, "2-byte block align"),
        (wav_u16(format, 14)? as u32, 16, "16-bit samples"),
    ];
    if let Some((_, _, contract)) = expected
        .iter()
        .find(|(actual, expected, _)| actual != expected)
    {
        return Err(AsrAudioError::Unsupported(format!("expected {contract}")));
    }

    let pcm = pcm.ok_or_else(|| AsrAudioError::Invalid("missing data chunk".into()))?;
    if pcm.is_empty() || pcm.len() & 1 != 0 {
        return Err(AsrAudioError::Invalid(
            "PCM data must contain complete samples".into(),
        ));
    }
    let mut samples = Vec::new();
    samples
        .try_reserve_exact(pcm.len() / 2)
        .map_err(|_| AsrAudioError::Invalid("PCM allocation failed".into()))?;
    for bytes in pcm.chunks_exact(2) {
        let sample = i16::from_le_bytes(bytes.try_into().unwrap()) as f32 / 32768.0;
        if !sample.is_finite() {
            return Err(AsrAudioError::Invalid("non-finite PCM sample".into()));
        }
        samples.push(sample);
    }

    #[cfg(feature = "parity-trace")]
    crate::parity_trace::report(crate::parity_trace::checkpoint(
        "asr.pcm",
        None,
        &[samples.len()],
        &samples,
    ));
    Ok(samples)
}

pub(crate) struct MelWindow {
    pub values: Vec<f32>,
    pub frames: usize,
    pub valid_frames: usize,
}

struct LogMel {
    raw: Vec<f32>,
    normalized: Vec<f32>,
    frames: usize,
}

fn zeroed_f32(len: usize) -> Result<Vec<f32>, AsrAudioError> {
    let mut values = Vec::new();
    values
        .try_reserve_exact(len)
        .map_err(|_| AsrAudioError::Invalid("audio allocation failed".into()))?;
    values.resize(len, 0.0);
    Ok(values)
}

fn reflect_pad(samples: &[f32]) -> Result<Vec<f32>, AsrAudioError> {
    let padded_len = samples
        .len()
        .checked_add(FFT_SIZE)
        .ok_or_else(|| AsrAudioError::Invalid("padded audio size overflow".into()))?;
    let mut padded = zeroed_f32(padded_len)?;
    let center_end = (FFT_SIZE / 2)
        .checked_add(samples.len())
        .ok_or_else(|| AsrAudioError::Invalid("padded audio range overflow".into()))?;
    padded[FFT_SIZE / 2..center_end].copy_from_slice(samples);
    for i in 0..FFT_SIZE / 2 {
        if FFT_SIZE / 2 - i < samples.len() {
            padded[i] = samples[FFT_SIZE / 2 - i];
        }
        if let Some(source) = samples.len().checked_sub(2 + i) {
            padded[center_end + i] = samples[source];
        }
    }
    Ok(padded)
}

fn slaney_mel_hz(mel: f32) -> f32 {
    let min_log_hz = 1_000.0;
    let min_log_mel = min_log_hz / (200.0 / 3.0);
    if mel >= min_log_mel {
        min_log_hz * ((mel - min_log_mel) * (6.4f32.ln() / 27.0)).exp()
    } else {
        mel * (200.0 / 3.0)
    }
}

fn mel_filters() -> Result<Vec<f32>, AsrAudioError> {
    let fft_bins = FFT_SIZE / 2 + 1;
    let filter_len = MEL_BINS
        .checked_mul(fft_bins)
        .ok_or_else(|| AsrAudioError::Invalid("Mel filter size overflow".into()))?;
    let mut filters = zeroed_f32(filter_len)?;
    let max_mel = 15.0 + (8.0f32).ln() / (6.4f32.ln() / 27.0);
    let mel_hz: Vec<f32> = (0..MEL_BINS + 2)
        .map(|i| slaney_mel_hz(max_mel * i as f32 / (MEL_BINS + 1) as f32))
        .collect();
    for mel in 0..MEL_BINS {
        let lower_width = mel_hz[mel + 1] - mel_hz[mel];
        let upper_width = mel_hz[mel + 2] - mel_hz[mel + 1];
        let norm = 2.0 / (mel_hz[mel + 2] - mel_hz[mel]);
        for bin in 0..fft_bins {
            let hz = bin as f32 * SAMPLE_RATE as f32 / FFT_SIZE as f32;
            let weight = ((hz - mel_hz[mel]) / lower_width)
                .min((mel_hz[mel + 2] - hz) / upper_width)
                .max(0.0)
                * norm;
            if !weight.is_finite() {
                return Err(AsrAudioError::Invalid("non-finite Mel filter".into()));
            }
            filters[mel * fft_bins + bin] = weight;
        }
    }
    Ok(filters)
}

fn compute_log_mel(samples: &[f32]) -> Result<LogMel, AsrAudioError> {
    if samples.is_empty() || samples.iter().any(|sample| !sample.is_finite()) {
        return Err(AsrAudioError::Invalid(
            "audio samples must be non-empty and finite".into(),
        ));
    }
    let padded = reflect_pad(samples)?;
    let stft_frames = padded
        .len()
        .checked_sub(FFT_SIZE)
        .and_then(|length| length.checked_div(HOP))
        .and_then(|frames| frames.checked_add(1))
        .ok_or_else(|| AsrAudioError::Invalid("STFT frame count overflow".into()))?;
    let sample_frames = samples
        .len()
        .checked_div(HOP)
        .and_then(|frames| frames.checked_add(1))
        .ok_or_else(|| AsrAudioError::Invalid("effective frame count overflow".into()))?;
    let frames = stft_frames.min(sample_frames);
    let raw_len = MEL_BINS
        .checked_mul(frames)
        .ok_or_else(|| AsrAudioError::Invalid("log-Mel size overflow".into()))?;
    let mut raw = zeroed_f32(raw_len)?;
    let filters = mel_filters()?;
    let fft_bins = FFT_SIZE / 2 + 1;
    let mut frame = Vec::new();
    frame
        .try_reserve_exact(FFT_SIZE)
        .map_err(|_| AsrAudioError::Invalid("FFT allocation failed".into()))?;
    frame.resize(FFT_SIZE, Complex::new(0.0, 0.0));
    let hann: Vec<f32> = (0..FFT_SIZE)
        .map(|i| 0.5 * (1.0 - (2.0 * std::f32::consts::PI * i as f32 / FFT_SIZE as f32).cos()))
        .collect();
    let mut planner = FftPlanner::<f32>::new();
    let fft = planner.plan_fft_forward(FFT_SIZE);

    for frame_index in 0..frames {
        let start = frame_index
            .checked_mul(HOP)
            .ok_or_else(|| AsrAudioError::Invalid("STFT offset overflow".into()))?;
        let end = start
            .checked_add(FFT_SIZE)
            .ok_or_else(|| AsrAudioError::Invalid("STFT range overflow".into()))?;
        let input = padded
            .get(start..end)
            .ok_or_else(|| AsrAudioError::Invalid("truncated padded frame".into()))?;
        for i in 0..FFT_SIZE {
            frame[i] = Complex::new(input[i] * hann[i], 0.0);
        }
        fft.process(&mut frame);
        if frame
            .iter()
            .any(|value| !value.re.is_finite() || !value.im.is_finite())
        {
            return Err(AsrAudioError::Invalid("non-finite FFT output".into()));
        }
        for mel in 0..MEL_BINS {
            let mut power_sum = 0.0;
            for bin in 0..fft_bins {
                power_sum += frame[bin].norm_sqr() * filters[mel * fft_bins + bin];
            }
            let value = power_sum.max(5.960464477539063e-8).log10();
            if !value.is_finite() {
                return Err(AsrAudioError::Invalid("non-finite log-Mel value".into()));
            }
            raw[mel * frames + frame_index] = value;
        }
    }

    let global_max = raw.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    if !global_max.is_finite() {
        return Err(AsrAudioError::Invalid("non-finite log-Mel maximum".into()));
    }
    let mut normalized = zeroed_f32(raw_len)?;
    for (normalized, value) in normalized.iter_mut().zip(&raw) {
        *normalized = (value.max(global_max - 8.0) + 4.0) / 4.0;
    }
    if normalized.iter().any(|value| !value.is_finite()) {
        return Err(AsrAudioError::Invalid(
            "non-finite normalized Mel value".into(),
        ));
    }

    #[cfg(feature = "parity-trace")]
    {
        crate::parity_trace::report(crate::parity_trace::checkpoint(
            "asr.raw_log_mel",
            None,
            &[MEL_BINS, frames],
            &raw,
        ));
        crate::parity_trace::report(crate::parity_trace::checkpoint(
            "asr.normalized_mel",
            None,
            &[MEL_BINS, frames],
            &normalized,
        ));
    }
    Ok(LogMel {
        raw,
        normalized,
        frames,
    })
}

fn split_mel_windows(normalized: &[f32], frames: usize) -> Result<Vec<MelWindow>, AsrAudioError> {
    let expected_len = MEL_BINS
        .checked_mul(frames)
        .ok_or_else(|| AsrAudioError::Invalid("normalized Mel size overflow".into()))?;
    if frames == 0
        || normalized.len() != expected_len
        || normalized.iter().any(|value| !value.is_finite())
    {
        return Err(AsrAudioError::Invalid(
            "invalid normalized Mel layout".into(),
        ));
    }

    let window_count = frames
        .checked_add(WINDOW_FRAMES - 1)
        .and_then(|value| value.checked_div(WINDOW_FRAMES))
        .ok_or_else(|| AsrAudioError::Invalid("Mel window count overflow".into()))?;
    let mut windows = Vec::new();
    windows
        .try_reserve_exact(window_count)
        .map_err(|_| AsrAudioError::Invalid("Mel window allocation failed".into()))?;
    let mut start = 0usize;
    while start < frames {
        let valid_frames = (frames - start).min(WINDOW_FRAMES);
        let padded_frames = valid_frames
            .checked_add(CHUNK_FRAMES - 1)
            .and_then(|value| value.checked_div(CHUNK_FRAMES))
            .and_then(|value| value.checked_mul(CHUNK_FRAMES))
            .ok_or_else(|| AsrAudioError::Invalid("padded Mel frame count overflow".into()))?;
        let values_len = MEL_BINS
            .checked_mul(padded_frames)
            .ok_or_else(|| AsrAudioError::Invalid("padded Mel size overflow".into()))?;
        let mut values = zeroed_f32(values_len)?;
        for mel in 0..MEL_BINS {
            let source_start = mel
                .checked_mul(frames)
                .and_then(|offset| offset.checked_add(start))
                .ok_or_else(|| AsrAudioError::Invalid("Mel source offset overflow".into()))?;
            let source_end = source_start
                .checked_add(valid_frames)
                .ok_or_else(|| AsrAudioError::Invalid("Mel source range overflow".into()))?;
            let destination_start = mel
                .checked_mul(padded_frames)
                .ok_or_else(|| AsrAudioError::Invalid("Mel destination offset overflow".into()))?;
            let destination_end = destination_start
                .checked_add(valid_frames)
                .ok_or_else(|| AsrAudioError::Invalid("Mel destination range overflow".into()))?;
            values[destination_start..destination_end]
                .copy_from_slice(&normalized[source_start..source_end]);
        }

        #[cfg(feature = "parity-trace")]
        crate::parity_trace::report(crate::parity_trace::checkpoint(
            "asr.padded_mel",
            None,
            &[MEL_BINS, padded_frames],
            &values,
        ));
        windows.push(MelWindow {
            values,
            frames: padded_frames,
            valid_frames,
        });
        start = start
            .checked_add(valid_frames)
            .ok_or_else(|| AsrAudioError::Invalid("Mel window offset overflow".into()))?;
    }
    Ok(windows)
}

pub(crate) fn log_mel_windows(samples: &[f32]) -> Result<Vec<MelWindow>, AsrAudioError> {
    let log_mel = compute_log_mel(samples)?;
    split_mel_windows(&log_mel.normalized, log_mel.frames)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Qwen3AudioConfig {
    pub hidden: usize,
    pub ffn: usize,
    pub layers: usize,
    pub heads: usize,
    pub mel_bins: usize,
    pub projection: usize,
    pub epsilon: f32,
}

impl Qwen3AudioConfig {
    pub fn from_source(source: &dyn TensorSource) -> Result<Self, String> {
        validate_qwen3a_source(source)
    }
}

fn require_string(source: &dyn TensorSource, key: &str, expected: &str) -> Result<(), String> {
    match source.metadata(key) {
        Some(MetaValue::String(value)) if value == expected => Ok(()),
        _ => Err(format!(
            "Invalid Qwen3A metadata {key}: expected {expected}"
        )),
    }
}

fn require_bool(source: &dyn TensorSource, key: &str, expected: bool) -> Result<(), String> {
    match source.metadata(key) {
        Some(MetaValue::Bool(value)) if *value == expected => Ok(()),
        _ => Err(format!(
            "Invalid Qwen3A metadata {key}: expected {expected}"
        )),
    }
}

fn require_u32(source: &dyn TensorSource, key: &str, expected: u32) -> Result<u32, String> {
    match source.metadata(key) {
        Some(MetaValue::Uint32(value)) if *value == expected => Ok(*value),
        _ => Err(format!(
            "Invalid Qwen3A metadata {key}: expected {expected}"
        )),
    }
}

fn require_f32(source: &dyn TensorSource, key: &str, expected: f32) -> Result<(), String> {
    match source.metadata(key) {
        Some(MetaValue::Float32(value)) if *value == expected => Ok(()),
        _ => Err(format!(
            "Invalid Qwen3A metadata {key}: expected {expected}"
        )),
    }
}

fn require_tensor(
    source: &dyn TensorSource,
    name: &str,
    dims: &[u64],
    ggml_type: GGMLType,
) -> Result<(), String> {
    let info = source
        .tensor_info(name)
        .ok_or_else(|| format!("Missing Qwen3A tensor: {name}"))?;
    if info.dims != dims || info.ggml_type != ggml_type {
        return Err(format!(
            "Invalid Qwen3A tensor {name}: shape {:?} type {:?}; expected {:?} {:?}",
            info.dims, info.ggml_type, dims, ggml_type
        ));
    }
    Ok(())
}

pub(crate) fn validate_qwen3a_source(
    source: &dyn TensorSource,
) -> Result<Qwen3AudioConfig, String> {
    require_string(source, "general.architecture", "clip")?;
    require_string(source, "general.type", "mmproj")?;
    require_bool(source, "clip.has_audio_encoder", true)?;
    require_string(source, "clip.audio.projector_type", "qwen3a")?;
    let hidden = usize::try_from(require_u32(source, "clip.audio.embedding_length", 896)?)
        .map_err(|_| "clip.audio.embedding_length does not fit usize")?;
    let ffn = usize::try_from(require_u32(source, "clip.audio.feed_forward_length", 3584)?)
        .map_err(|_| "clip.audio.feed_forward_length does not fit usize")?;
    let layers = usize::try_from(require_u32(source, "clip.audio.block_count", 18)?)
        .map_err(|_| "clip.audio.block_count does not fit usize")?;
    let heads = usize::try_from(require_u32(source, "clip.audio.attention.head_count", 14)?)
        .map_err(|_| "clip.audio.attention.head_count does not fit usize")?;
    let mel_bins = usize::try_from(require_u32(source, "clip.audio.num_mel_bins", 128)?)
        .map_err(|_| "clip.audio.num_mel_bins does not fit usize")?;
    let projection = usize::try_from(require_u32(source, "clip.audio.projection_dim", 1024)?)
        .map_err(|_| "clip.audio.projection_dim does not fit usize")?;
    require_f32(source, "clip.audio.attention.layer_norm_epsilon", 1e-5)?;

    for i in 0..18 {
        let prefix = format!("a.blk.{i}");
        for name in ["attn_q", "attn_k", "attn_v", "attn_out"] {
            require_tensor(
                source,
                &format!("{prefix}.{name}.weight"),
                &[896, 896],
                GGMLType::Q8_0,
            )?;
            require_tensor(
                source,
                &format!("{prefix}.{name}.bias"),
                &[896],
                GGMLType::F32,
            )?;
        }
        for name in ["ln1", "ln2"] {
            require_tensor(
                source,
                &format!("{prefix}.{name}.weight"),
                &[896],
                GGMLType::F32,
            )?;
            require_tensor(
                source,
                &format!("{prefix}.{name}.bias"),
                &[896],
                GGMLType::F32,
            )?;
        }
        require_tensor(
            source,
            &format!("{prefix}.ffn_up.weight"),
            &[896, 3584],
            GGMLType::Q8_0,
        )?;
        require_tensor(
            source,
            &format!("{prefix}.ffn_up.bias"),
            &[3584],
            GGMLType::F32,
        )?;
        require_tensor(
            source,
            &format!("{prefix}.ffn_down.weight"),
            &[3584, 896],
            GGMLType::Q8_0,
        )?;
        require_tensor(
            source,
            &format!("{prefix}.ffn_down.bias"),
            &[896],
            GGMLType::F32,
        )?;
    }
    for (name, dims, ggml_type) in [
        ("a.position_embd.weight", &[896, 1500][..], GGMLType::F32),
        ("a.conv2d.1.weight", &[3, 3, 1, 480][..], GGMLType::F16),
        ("a.conv2d.1.bias", &[1, 1, 480][..], GGMLType::F32),
        ("a.conv2d.2.weight", &[3, 3, 480, 480][..], GGMLType::F16),
        ("a.conv2d.2.bias", &[1, 1, 480][..], GGMLType::F32),
        ("a.conv2d.3.weight", &[3, 3, 480, 480][..], GGMLType::F16),
        ("a.conv2d.3.bias", &[1, 1, 480][..], GGMLType::F32),
        ("a.conv_out.weight", &[7680, 896][..], GGMLType::F16),
        ("a.post_ln.weight", &[896][..], GGMLType::F32),
        ("a.post_ln.bias", &[896][..], GGMLType::F32),
        ("mm.a.mlp.1.weight", &[896, 896][..], GGMLType::Q8_0),
        ("mm.a.mlp.1.bias", &[896][..], GGMLType::F32),
        ("mm.a.mlp.2.weight", &[896, 1024][..], GGMLType::Q8_0),
        ("mm.a.mlp.2.bias", &[1024][..], GGMLType::F32),
    ] {
        require_tensor(source, name, dims, ggml_type)?;
    }

    Ok(Qwen3AudioConfig {
        hidden,
        ffn,
        layers,
        heads,
        mel_bins,
        projection,
        epsilon: 1e-5,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{GGMLType, MetaValue, TensorInfo, TensorSource};
    use std::collections::HashMap;

    fn append_wav_chunk(bytes: &mut Vec<u8>, id: &[u8; 4], chunk: &[u8]) {
        bytes.extend_from_slice(id);
        bytes.extend_from_slice(&(chunk.len() as u32).to_le_bytes());
        bytes.extend_from_slice(chunk);
        if chunk.len() & 1 != 0 {
            bytes.push(0);
        }
    }

    fn pcm16_wav(samples: &[i16], extra_chunks: &[(&[u8; 4], Vec<u8>)]) -> Vec<u8> {
        let mut bytes = b"RIFF\0\0\0\0WAVE".to_vec();
        let mut format = Vec::new();
        format.extend_from_slice(&1u16.to_le_bytes());
        format.extend_from_slice(&1u16.to_le_bytes());
        format.extend_from_slice(&16_000u32.to_le_bytes());
        format.extend_from_slice(&32_000u32.to_le_bytes());
        format.extend_from_slice(&2u16.to_le_bytes());
        format.extend_from_slice(&16u16.to_le_bytes());
        append_wav_chunk(&mut bytes, b"fmt ", &format);
        for (id, chunk) in extra_chunks {
            append_wav_chunk(&mut bytes, id, chunk);
        }
        let pcm: Vec<u8> = samples
            .iter()
            .flat_map(|sample| sample.to_le_bytes())
            .collect();
        append_wav_chunk(&mut bytes, b"data", &pcm);
        let riff_len = u32::try_from(bytes.len() - 8).unwrap();
        bytes[4..8].copy_from_slice(&riff_len.to_le_bytes());
        bytes
    }

    fn reference_pad(samples: &[f32]) -> Vec<f32> {
        let mut padded = vec![0.0; samples.len() + 400];
        padded[200..200 + samples.len()].copy_from_slice(samples);
        for i in 0..200 {
            if 200 - i < samples.len() {
                padded[i] = samples[200 - i];
            }
            if let Some(source) = samples.len().checked_sub(2 + i) {
                padded[200 + samples.len() + i] = samples[source];
            }
        }
        padded
    }

    fn reference_mel_hz(mel: f64) -> f64 {
        let min_log_hz = 1_000.0;
        let min_log_mel = min_log_hz / (200.0 / 3.0);
        if mel >= min_log_mel {
            min_log_hz * ((mel - min_log_mel) * (6.4f64.ln() / 27.0)).exp()
        } else {
            mel * (200.0 / 3.0)
        }
    }

    fn reference_log_mel_frame(samples: &[f32], frame: usize) -> Vec<f32> {
        let padded = reference_pad(samples);
        let start = frame * 160;
        let mut power = vec![0.0f64; 201];
        for (bin, output) in power.iter_mut().enumerate() {
            let mut real = 0.0;
            let mut imaginary = 0.0;
            for (i, sample) in padded[start..start + 400].iter().enumerate() {
                let window = 0.5f64 * (1.0 - (2.0 * std::f64::consts::PI * i as f64 / 400.0).cos());
                let angle = -2.0 * std::f64::consts::PI * bin as f64 * i as f64 / 400.0;
                real += f64::from(*sample) * window * angle.cos();
                imaginary += f64::from(*sample) * window * angle.sin();
            }
            *output = real * real + imaginary * imaginary;
        }

        let max_mel = 15.0 + (8.0f64).ln() / (6.4f64.ln() / 27.0);
        let mel_hz: Vec<f64> = (0..130)
            .map(|i| reference_mel_hz(max_mel * i as f64 / 129.0))
            .collect();
        (0..128)
            .map(|mel| {
                let lower_width = mel_hz[mel + 1] - mel_hz[mel];
                let upper_width = mel_hz[mel + 2] - mel_hz[mel + 1];
                let norm = 2.0 / (mel_hz[mel + 2] - mel_hz[mel]);
                let sum = power
                    .iter()
                    .enumerate()
                    .map(|(bin, value)| {
                        let hz = bin as f64 * 16_000.0 / 400.0;
                        let weight = ((hz - mel_hz[mel]) / lower_width)
                            .min((mel_hz[mel + 2] - hz) / upper_width)
                            .max(0.0);
                        value * weight * norm
                    })
                    .sum::<f64>();
                sum.max(5.960464477539063e-8).log10() as f32
            })
            .collect()
    }

    #[test]
    fn wav_valid_pcm_and_unknown_chunks_decode() {
        let bytes = pcm16_wav(&[-32768, 0, 32767], &[(b"JUNK", vec![7])]);
        let samples = decode_pcm16_wav(&bytes).unwrap();
        assert_eq!(samples, vec![-1.0, 0.0, 32767.0 / 32768.0]);

        let bytes = pcm16_wav(&[1], &[(b"LIST", vec![1, 2]), (b"JUNK", vec![7])]);
        assert_eq!(decode_pcm16_wav(&bytes).unwrap(), vec![1.0 / 32768.0]);
    }

    #[test]
    fn wav_truncated_headers_chunks_and_padding_are_invalid() {
        let mut riff = pcm16_wav(&[1], &[]);
        riff.pop();
        assert!(matches!(
            decode_pcm16_wav(&riff),
            Err(AsrAudioError::Invalid(_))
        ));

        let mut header = b"RIFF\0\0\0\0WAVEfmt ".to_vec();
        let len = u32::try_from(header.len() - 8).unwrap();
        header[4..8].copy_from_slice(&len.to_le_bytes());
        assert!(matches!(
            decode_pcm16_wav(&header),
            Err(AsrAudioError::Invalid(_))
        ));

        let mut chunk = b"RIFF\0\0\0\0WAVEdata\x08\0\0\0\x01".to_vec();
        let len = u32::try_from(chunk.len() - 8).unwrap();
        chunk[4..8].copy_from_slice(&len.to_le_bytes());
        assert!(matches!(
            decode_pcm16_wav(&chunk),
            Err(AsrAudioError::Invalid(_))
        ));

        let mut padding = b"RIFF\0\0\0\0WAVEJUNK\x01\0\0\0\x01".to_vec();
        let len = u32::try_from(padding.len() - 8).unwrap();
        padding[4..8].copy_from_slice(&len.to_le_bytes());
        assert!(matches!(
            decode_pcm16_wav(&padding),
            Err(AsrAudioError::Invalid(_))
        ));
    }

    #[test]
    fn wav_duplicate_required_chunks_are_invalid() {
        let format = vec![1, 0, 1, 0, 0x80, 0x3e, 0, 0, 0, 0x7d, 0, 0, 2, 0, 16, 0];
        assert!(matches!(
            decode_pcm16_wav(&pcm16_wav(&[1], &[(b"fmt ", format)])),
            Err(AsrAudioError::Invalid(_))
        ));
        assert!(matches!(
            decode_pcm16_wav(&pcm16_wav(&[1], &[(b"data", vec![0, 0])])),
            Err(AsrAudioError::Invalid(_))
        ));
    }

    #[test]
    fn wav_unsupported_format_contract_is_rejected() {
        for (offset, bytes) in [
            (20, 3u16.to_le_bytes().to_vec()),
            (22, 2u16.to_le_bytes().to_vec()),
            (24, 8_000u32.to_le_bytes().to_vec()),
            (28, 16_000u32.to_le_bytes().to_vec()),
            (32, 4u16.to_le_bytes().to_vec()),
            (34, 8u16.to_le_bytes().to_vec()),
        ] {
            let mut wav = pcm16_wav(&[1], &[]);
            wav[offset..offset + bytes.len()].copy_from_slice(&bytes);
            assert!(matches!(
                decode_pcm16_wav(&wav),
                Err(AsrAudioError::Unsupported(_))
            ));
        }

        assert!(matches!(
            decode_pcm16_wav(b"not a wave"),
            Err(AsrAudioError::Unsupported(_))
        ));
    }

    #[test]
    fn wav_odd_and_empty_pcm_are_invalid() {
        let mut odd = pcm16_wav(&[1], &[]);
        odd[40..44].copy_from_slice(&1u32.to_le_bytes());
        assert!(matches!(
            decode_pcm16_wav(&odd),
            Err(AsrAudioError::Invalid(_))
        ));
        assert!(matches!(
            decode_pcm16_wav(&pcm16_wav(&[], &[])),
            Err(AsrAudioError::Invalid(_))
        ));
    }

    #[test]
    fn silence_impulse_and_440hz_have_pinned_mel_shapes() {
        let silence = vec![0.0; 16_000];
        let impulse = {
            let mut values = vec![0.0; 16_000];
            values[8_000] = 1.0;
            values
        };
        let tone: Vec<f32> = (0..16_000)
            .map(|i| (2.0 * std::f32::consts::PI * 440.0 * i as f32 / 16_000.0).sin())
            .collect();

        for (samples, reference_frame) in [(&silence, 0), (&impulse, 50), (&tone, 0)] {
            let expected = reference_log_mel_frame(samples, reference_frame);
            let actual = compute_log_mel(samples).unwrap();
            assert_eq!(actual.frames, 101);
            assert_eq!(actual.raw.len(), 128 * 101);
            assert_eq!(actual.normalized.len(), 128 * 101);
            assert!(actual
                .raw
                .iter()
                .chain(&actual.normalized)
                .all(|value| value.is_finite()));
            for mel in 0..128 {
                assert!(
                    (actual.raw[mel * actual.frames + reference_frame] - expected[mel]).abs()
                        <= 1e-5,
                    "mel {mel}: actual {}, expected {}",
                    actual.raw[mel * actual.frames + reference_frame],
                    expected[mel]
                );
            }
            let global_max = actual.raw.iter().copied().fold(f32::NEG_INFINITY, f32::max);
            for (raw, normalized) in actual.raw.iter().zip(&actual.normalized) {
                assert_eq!(*normalized, (raw.max(global_max - 8.0) + 4.0) / 4.0);
            }
        }
    }

    #[test]
    fn short_audio_reflection_padding_uses_the_reference_zero_fallback() {
        for samples in [
            &[7.0][..],
            &(0..199).map(|i| i as f32).collect::<Vec<_>>()[..],
        ] {
            let actual = reflect_pad(samples).unwrap();
            assert_eq!(actual, reference_pad(samples));
            for i in 0..200 {
                let expected_start = samples.get(200 - i).copied().unwrap_or(0.0);
                let expected_end = samples
                    .len()
                    .checked_sub(2 + i)
                    .map(|source| samples[source])
                    .unwrap_or(0.0);
                assert_eq!(actual[i], expected_start);
                assert_eq!(actual[200 + samples.len() + i], expected_end);
            }
            let log_mel = compute_log_mel(samples).unwrap();
            assert_eq!(log_mel.frames, samples.len() / 160 + 1);
            assert!(log_mel.raw.iter().all(|value| value.is_finite()));
        }
    }

    #[test]
    fn mel_windows_use_800_100_boundaries_and_zero_padding() {
        for (frames, expected_frames) in [
            (1, vec![100]),
            (100, vec![100]),
            (101, vec![200]),
            (800, vec![800]),
            (801, vec![800, 100]),
        ] {
            let normalized: Vec<f32> = (0..128)
                .flat_map(|mel| (0..frames).map(move |frame| (mel * 10_000 + frame + 1) as f32))
                .collect();
            let windows = split_mel_windows(&normalized, frames).unwrap();
            assert_eq!(
                windows
                    .iter()
                    .map(|window| window.frames)
                    .collect::<Vec<_>>(),
                expected_frames
            );

            let mut source_frame = 0;
            for window in windows {
                assert_eq!(window.values.len(), 128 * window.frames);
                for mel in 0..128 {
                    for frame in 0..window.valid_frames {
                        assert_eq!(
                            window.values[mel * window.frames + frame],
                            normalized[mel * frames + source_frame + frame]
                        );
                    }
                    assert!(window.values
                        [mel * window.frames + window.valid_frames..(mel + 1) * window.frames]
                        .iter()
                        .all(|value| *value == 0.0));
                }
                source_frame += window.valid_frames;
            }
            assert_eq!(source_frame, frames);
        }

        assert!(matches!(
            log_mel_windows(&[]),
            Err(AsrAudioError::Invalid(_))
        ));
        assert!(matches!(
            log_mel_windows(&[f32::NAN]),
            Err(AsrAudioError::Invalid(_))
        ));
    }

    #[derive(Default)]
    struct MapTensorSource {
        metadata: HashMap<String, MetaValue>,
        tensors: HashMap<String, TensorInfo>,
    }

    impl TensorSource for MapTensorSource {
        fn metadata(&self, key: &str) -> Option<&MetaValue> {
            self.metadata.get(key)
        }

        fn tensor_info(&self, name: &str) -> Option<&TensorInfo> {
            self.tensors.get(name)
        }

        fn tensor_slice(&self, _name: &str) -> Option<&[u8]> {
            None
        }
    }

    fn add_tensor(
        source: &mut MapTensorSource,
        name: impl Into<String>,
        dims: &[u64],
        ggml_type: GGMLType,
    ) {
        let name = name.into();
        source.tensors.insert(
            name.clone(),
            TensorInfo {
                name,
                dims: dims.to_vec(),
                ggml_type,
                offset: 0,
            },
        );
    }

    fn valid_qwen3a_source() -> MapTensorSource {
        let mut source = MapTensorSource {
            metadata: HashMap::from([
                (
                    "general.architecture".into(),
                    MetaValue::String("clip".into()),
                ),
                ("general.type".into(), MetaValue::String("mmproj".into())),
                ("clip.has_audio_encoder".into(), MetaValue::Bool(true)),
                (
                    "clip.audio.projector_type".into(),
                    MetaValue::String("qwen3a".into()),
                ),
                ("clip.audio.embedding_length".into(), MetaValue::Uint32(896)),
                (
                    "clip.audio.feed_forward_length".into(),
                    MetaValue::Uint32(3584),
                ),
                ("clip.audio.block_count".into(), MetaValue::Uint32(18)),
                (
                    "clip.audio.attention.head_count".into(),
                    MetaValue::Uint32(14),
                ),
                ("clip.audio.num_mel_bins".into(), MetaValue::Uint32(128)),
                ("clip.audio.projection_dim".into(), MetaValue::Uint32(1024)),
                (
                    "clip.audio.attention.layer_norm_epsilon".into(),
                    MetaValue::Float32(1e-5),
                ),
            ]),
            tensors: HashMap::new(),
        };
        for i in 0..18 {
            let prefix = format!("a.blk.{i}");
            for name in ["attn_q", "attn_k", "attn_v", "attn_out"] {
                add_tensor(
                    &mut source,
                    format!("{prefix}.{name}.weight"),
                    &[896, 896],
                    GGMLType::Q8_0,
                );
                add_tensor(
                    &mut source,
                    format!("{prefix}.{name}.bias"),
                    &[896],
                    GGMLType::F32,
                );
            }
            for name in ["ln1", "ln2"] {
                add_tensor(
                    &mut source,
                    format!("{prefix}.{name}.weight"),
                    &[896],
                    GGMLType::F32,
                );
                add_tensor(
                    &mut source,
                    format!("{prefix}.{name}.bias"),
                    &[896],
                    GGMLType::F32,
                );
            }
            add_tensor(
                &mut source,
                format!("{prefix}.ffn_up.weight"),
                &[896, 3584],
                GGMLType::Q8_0,
            );
            add_tensor(
                &mut source,
                format!("{prefix}.ffn_up.bias"),
                &[3584],
                GGMLType::F32,
            );
            add_tensor(
                &mut source,
                format!("{prefix}.ffn_down.weight"),
                &[3584, 896],
                GGMLType::Q8_0,
            );
            add_tensor(
                &mut source,
                format!("{prefix}.ffn_down.bias"),
                &[896],
                GGMLType::F32,
            );
        }
        for (name, dims, ggml_type) in [
            ("a.position_embd.weight", &[896, 1500][..], GGMLType::F32),
            ("a.conv2d.1.weight", &[3, 3, 1, 480][..], GGMLType::F16),
            ("a.conv2d.1.bias", &[1, 1, 480][..], GGMLType::F32),
            ("a.conv2d.2.weight", &[3, 3, 480, 480][..], GGMLType::F16),
            ("a.conv2d.2.bias", &[1, 1, 480][..], GGMLType::F32),
            ("a.conv2d.3.weight", &[3, 3, 480, 480][..], GGMLType::F16),
            ("a.conv2d.3.bias", &[1, 1, 480][..], GGMLType::F32),
            ("a.conv_out.weight", &[7680, 896][..], GGMLType::F16),
            ("a.post_ln.weight", &[896][..], GGMLType::F32),
            ("a.post_ln.bias", &[896][..], GGMLType::F32),
            ("mm.a.mlp.1.weight", &[896, 896][..], GGMLType::Q8_0),
            ("mm.a.mlp.1.bias", &[896][..], GGMLType::F32),
            ("mm.a.mlp.2.weight", &[896, 1024][..], GGMLType::Q8_0),
            ("mm.a.mlp.2.bias", &[1024][..], GGMLType::F32),
        ] {
            add_tensor(&mut source, name, dims, ggml_type);
        }
        source
    }

    #[test]
    fn qwen3a_contract_accepts_only_the_fixed_model() {
        let expected = Qwen3AudioConfig {
            hidden: 896,
            ffn: 3584,
            layers: 18,
            heads: 14,
            mel_bins: 128,
            projection: 1024,
            epsilon: 1e-5,
        };
        assert_eq!(
            Qwen3AudioConfig::from_source(&valid_qwen3a_source()).unwrap(),
            expected
        );
    }

    #[test]
    fn qwen3a_contract_rejects_metadata_shape_and_type_drift() {
        let mut missing_metadata = valid_qwen3a_source();
        missing_metadata
            .metadata
            .remove("clip.audio.embedding_length");
        assert!(validate_qwen3a_source(&missing_metadata)
            .unwrap_err()
            .contains("clip.audio.embedding_length"));

        let mut wrong_projector = valid_qwen3a_source();
        wrong_projector.metadata.insert(
            "clip.audio.projector_type".into(),
            MetaValue::String("other".into()),
        );
        assert!(validate_qwen3a_source(&wrong_projector)
            .unwrap_err()
            .contains("clip.audio.projector_type"));

        let mut missing_tensor = valid_qwen3a_source();
        missing_tensor.tensors.remove("a.blk.0.attn_q.weight");
        assert!(validate_qwen3a_source(&missing_tensor)
            .unwrap_err()
            .contains("a.blk.0.attn_q.weight"));

        let mut wrong_shape = valid_qwen3a_source();
        wrong_shape
            .tensors
            .get_mut("a.conv_out.weight")
            .unwrap()
            .dims = vec![896, 7680];
        assert!(validate_qwen3a_source(&wrong_shape)
            .unwrap_err()
            .contains("a.conv_out.weight"));

        let mut wrong_type = valid_qwen3a_source();
        wrong_type
            .tensors
            .get_mut("a.post_ln.weight")
            .unwrap()
            .ggml_type = GGMLType::F16;
        assert!(validate_qwen3a_source(&wrong_type)
            .unwrap_err()
            .contains("a.post_ln.weight"));

        let mut wrong_projection = valid_qwen3a_source();
        wrong_projection
            .metadata
            .insert("clip.audio.projection_dim".into(), MetaValue::Uint32(512));
        assert!(validate_qwen3a_source(&wrong_projection)
            .unwrap_err()
            .contains("clip.audio.projection_dim"));
    }
}
