use crate::model::{GGMLType, MetaValue, TensorSource};
use crate::ops::f16_to_f32;
use rustfft::{num_complex::Complex, FftPlanner};
use std::sync::Arc;

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
    if bytes.get(0..4) != Some(b"RIFF") {
        return Err(AsrAudioError::Unsupported("expected RIFF/WAVE".into()));
    }
    let wave = bytes
        .get(8..12)
        .ok_or_else(|| AsrAudioError::Invalid("truncated RIFF/WAVE header".into()))?;
    if wave != b"WAVE" {
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
                let power = frame[bin].norm_sqr();
                if !power.is_finite() {
                    return Err(AsrAudioError::Invalid("non-finite FFT power".into()));
                }
                let weighted_power = power * filters[mel * fft_bins + bin];
                if !weighted_power.is_finite() {
                    return Err(AsrAudioError::Invalid(
                        "non-finite weighted Mel power".into(),
                    ));
                }
                power_sum += weighted_power;
                if !power_sum.is_finite() {
                    return Err(AsrAudioError::Invalid("non-finite Mel power sum".into()));
                }
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

struct F16Tensor {
    bytes: &'static [u8],
    dims: Vec<u64>,
}

struct AudioLinear {
    weight: &'static [u8],
    kind: GGMLType,
    input: usize,
    output: usize,
    bias: Vec<f32>,
}

struct Conv2dWeights {
    weight: F16Tensor,
    bias: Vec<f32>,
    input_channels: usize,
    output_channels: usize,
}

struct AudioHidden {
    values: Vec<f32>,
    tokens: usize,
}

struct Qwen3AudioModel {
    source: Arc<dyn TensorSource>,
    config: Qwen3AudioConfig,
    conv: [Conv2dWeights; 3],
    conv_out: AudioLinear,
}

impl Qwen3AudioModel {
    fn from_source(source: Arc<dyn TensorSource>) -> Result<Self, String> {
        let config = Qwen3AudioConfig::from_source(source.as_ref())?;
        let conv = [
            load_conv2d(&source, "a.conv2d.1", 1, 480)?,
            load_conv2d(&source, "a.conv2d.2", 480, 480)?,
            load_conv2d(&source, "a.conv2d.3", 480, 480)?,
        ];
        let conv_out = AudioLinear::load(
            &source,
            "a.conv_out.weight",
            None,
            7680,
            config.hidden,
            GGMLType::F16,
        )?;
        Ok(Self {
            source,
            config,
            conv,
            conv_out,
        })
    }

    fn encode_convolution(&self, window: &MelWindow) -> Result<AudioHidden, String> {
        if window.frames == 0
            || window.frames > WINDOW_FRAMES
            || window.frames % CHUNK_FRAMES != 0
            || window.valid_frames == 0
            || window.valid_frames > window.frames
        {
            return Err("Invalid Mel window frame count".into());
        }
        let expected = checked_product("Mel window values", MEL_BINS, window.frames)?;
        if window.values.len() != expected || window.values.iter().any(|value| !value.is_finite()) {
            return Err("Invalid Mel window values".into());
        }

        let chunks = window.frames / CHUNK_FRAMES;
        let tokens = checked_product("convolution tokens", chunks, 13)?;
        let hidden_len = checked_product("convolution hidden values", tokens, self.config.hidden)?;
        let mut hidden = Vec::new();
        hidden
            .try_reserve_exact(hidden_len)
            .map_err(|_| "Failed to allocate convolution hidden values".to_string())?;
        let mut chunk = reserved_f32(
            "Mel convolution chunk",
            checked_product("Mel convolution chunk", MEL_BINS, CHUNK_FRAMES)?,
        )?;
        let mut stage_a = Vec::new();
        let mut stage_b = Vec::new();
        let mut flattened = Vec::new();
        let mut projected = Vec::new();

        for chunk_index in 0..chunks {
            for mel in 0..MEL_BINS {
                let source_start = checked_product("Mel chunk source", mel, window.frames)?
                    .checked_add(checked_product(
                        "Mel chunk offset",
                        chunk_index,
                        CHUNK_FRAMES,
                    )?)
                    .ok_or_else(|| "Mel chunk source range overflow".to_string())?;
                let source_end = source_start
                    .checked_add(CHUNK_FRAMES)
                    .ok_or_else(|| "Mel chunk source range overflow".to_string())?;
                let destination_start =
                    checked_product("Mel chunk destination", mel, CHUNK_FRAMES)?;
                chunk[destination_start..destination_start + CHUNK_FRAMES]
                    .copy_from_slice(&window.values[source_start..source_end]);
            }

            let (height, width) = conv2d_stride2_padding1(
                &chunk,
                1,
                MEL_BINS,
                CHUNK_FRAMES,
                &self.conv[0],
                &mut stage_a,
            )?;
            apply_gelu(&mut stage_a)?;
            let (height, width) =
                conv2d_stride2_padding1(&stage_a, 480, height, width, &self.conv[1], &mut stage_b)?;
            apply_gelu(&mut stage_b)?;
            let (height, width) =
                conv2d_stride2_padding1(&stage_b, 480, height, width, &self.conv[2], &mut stage_a)?;
            apply_gelu(&mut stage_a)?;
            if (height, width) != (16, 13) {
                return Err(format!(
                    "Invalid final convolution shape: [1,480,{height},{width}]"
                ));
            }

            #[cfg(feature = "parity-trace")]
            crate::parity_trace::report(crate::parity_trace::checkpoint(
                "asr.after_conv_blocks",
                None,
                &[1, 480, height, width],
                &stage_a,
            ));

            flatten_conv_output(&stage_a, 480, height, width, &mut flattened)?;
            self.conv_out
                .project_f16(&flattened, width, &mut projected)?;
            #[cfg(feature = "parity-trace")]
            crate::parity_trace::report(crate::parity_trace::checkpoint(
                "asr.after_conv_out",
                None,
                &[width, self.config.hidden],
                &projected,
            ));
            hidden.extend_from_slice(&projected);
        }
        if hidden.len() != hidden_len || hidden.iter().any(|value| !value.is_finite()) {
            return Err("Invalid convolution hidden output".into());
        }
        Ok(AudioHidden {
            values: hidden,
            tokens,
        })
    }
}

impl AudioLinear {
    fn load(
        source: &Arc<dyn TensorSource>,
        weight_name: &str,
        bias_name: Option<&str>,
        input: usize,
        output: usize,
        kind: GGMLType,
    ) -> Result<Self, String> {
        let allowed = (weight_name == "a.conv_out.weight" && kind == GGMLType::F16)
            || (kind == GGMLType::Q8_0 && is_q8_audio_linear(weight_name));
        if !allowed {
            return Err(format!(
                "Unsupported audio linear tensor {weight_name} type {kind:?}"
            ));
        }
        let dims = [
            to_u64(input, "audio linear input")?,
            to_u64(output, "audio linear output")?,
        ];
        let weight = static_tensor(source, weight_name, &dims, kind)?;
        let bias = match bias_name {
            Some(name) => load_f32_tensor(source.as_ref(), name, &[dims[1]])?,
            None => Vec::new(),
        };
        Ok(Self {
            weight,
            kind,
            input,
            output,
            bias,
        })
    }

    fn project_f16(&self, input: &[f32], rows: usize, result: &mut Vec<f32>) -> Result<(), String> {
        if self.kind != GGMLType::F16 || !self.bias.is_empty() {
            return Err("Convolution projection must be bias-free F16".into());
        }
        let input_len = checked_product("audio projection input", rows, self.input)?;
        if input.len() != input_len || input.iter().any(|value| !value.is_finite()) {
            return Err("Invalid audio projection input".into());
        }
        let output_len = checked_product("audio projection output", rows, self.output)?;
        resize_f32(result, "audio projection output", output_len)?;
        result.fill(0.0);
        if input.iter().all(|value| *value == 0.0) {
            return Ok(());
        }
        for row in 0..rows {
            let input_row = &input[row * self.input..(row + 1) * self.input];
            for output in 0..self.output {
                let weight_start = checked_product("audio projection weight", output, self.input)?;
                let mut sum = 0.0;
                for (column, value) in input_row.iter().enumerate() {
                    let byte =
                        checked_product("audio projection weight byte", weight_start + column, 2)?;
                    let bits = u16::from_le_bytes([self.weight[byte], self.weight[byte + 1]]);
                    sum += *value * f16_to_f32(bits);
                }
                if !sum.is_finite() {
                    return Err("Non-finite audio projection output".into());
                }
                result[row * self.output + output] = sum;
            }
        }
        Ok(())
    }
}

fn is_q8_audio_linear(name: &str) -> bool {
    if matches!(name, "mm.a.mlp.1.weight" | "mm.a.mlp.2.weight") {
        return true;
    }
    let Some((layer, suffix)) = name
        .strip_prefix("a.blk.")
        .and_then(|name| name.split_once('.'))
    else {
        return false;
    };
    layer.parse::<usize>().is_ok_and(|layer| layer < 18)
        && matches!(
            suffix,
            "attn_q.weight"
                | "attn_k.weight"
                | "attn_v.weight"
                | "attn_out.weight"
                | "ffn_up.weight"
                | "ffn_down.weight"
        )
}

fn load_conv2d(
    source: &Arc<dyn TensorSource>,
    prefix: &str,
    input_channels: usize,
    output_channels: usize,
) -> Result<Conv2dWeights, String> {
    let dims = vec![
        3,
        3,
        to_u64(input_channels, "convolution input channels")?,
        to_u64(output_channels, "convolution output channels")?,
    ];
    let weight = F16Tensor {
        bytes: static_tensor(source, &format!("{prefix}.weight"), &dims, GGMLType::F16)?,
        dims,
    };
    let bias = load_f32_tensor(
        source.as_ref(),
        &format!("{prefix}.bias"),
        &[1, 1, to_u64(output_channels, "convolution bias channels")?],
    )?;
    Ok(Conv2dWeights {
        weight,
        bias,
        input_channels,
        output_channels,
    })
}

fn conv2d_stride2_padding1(
    input: &[f32],
    input_channels: usize,
    input_height: usize,
    input_width: usize,
    weights: &Conv2dWeights,
    output: &mut Vec<f32>,
) -> Result<(usize, usize), String> {
    if input_channels == 0 || input_height == 0 || input_width == 0 {
        return Err("Convolution input dimensions must be non-zero".into());
    }
    let input_len = checked_product(
        "convolution input",
        checked_product("convolution input plane", input_channels, input_height)?,
        input_width,
    )?;
    let expected_dims = [
        3,
        3,
        to_u64(input_channels, "convolution input channels")?,
        to_u64(weights.output_channels, "convolution output channels")?,
    ];
    let weight_elements = checked_product(
        "convolution weights",
        checked_product("convolution kernel channels", 9, input_channels)?,
        weights.output_channels,
    )?;
    if input.len() != input_len
        || input.iter().any(|value| !value.is_finite())
        || weights.input_channels != input_channels
        || weights.weight.dims != expected_dims
        || weights.weight.bytes.len()
            != checked_product("convolution weight bytes", weight_elements, 2)?
        || weights.bias.len() != weights.output_channels
        || weights.bias.iter().any(|value| !value.is_finite())
    {
        return Err("Invalid convolution tensor layout".into());
    }
    let output_height = input_height
        .checked_add(1)
        .ok_or_else(|| "convolution output height overflow".to_string())?
        / 2;
    let output_width = input_width
        .checked_add(1)
        .ok_or_else(|| "convolution output width overflow".to_string())?
        / 2;
    let output_len = checked_product(
        "convolution output",
        checked_product(
            "convolution output plane",
            weights.output_channels,
            output_height,
        )?,
        output_width,
    )?;
    resize_f32(output, "convolution output", output_len)?;

    for output_channel in 0..weights.output_channels {
        let plane_start = checked_product(
            "convolution output channel",
            output_channel,
            checked_product("convolution output spatial", output_height, output_width)?,
        )?;
        output[plane_start..plane_start + output_height * output_width]
            .fill(weights.bias[output_channel]);
    }
    if input.iter().all(|value| *value == 0.0) {
        return Ok((output_height, output_width));
    }

    for output_channel in 0..weights.output_channels {
        for output_y in 0..output_height {
            for output_x in 0..output_width {
                let mut sum = weights.bias[output_channel];
                for input_channel in 0..input_channels {
                    for kernel_y in 0..3 {
                        let padded_y = output_y * 2 + kernel_y;
                        if padded_y == 0 || padded_y > input_height {
                            continue;
                        }
                        let input_y = padded_y - 1;
                        for kernel_x in 0..3 {
                            let padded_x = output_x * 2 + kernel_x;
                            if padded_x == 0 || padded_x > input_width {
                                continue;
                            }
                            let input_x = padded_x - 1;
                            let input_index =
                                (input_channel * input_height + input_y) * input_width + input_x;
                            let weight_index =
                                (((output_channel * input_channels + input_channel) * 3
                                    + kernel_y)
                                    * 3)
                                    + kernel_x;
                            let byte = weight_index * 2;
                            let bits = u16::from_le_bytes([
                                weights.weight.bytes[byte],
                                weights.weight.bytes[byte + 1],
                            ]);
                            sum += input[input_index] * f16_to_f32(bits);
                        }
                    }
                }
                if !sum.is_finite() {
                    return Err("Non-finite convolution output".into());
                }
                output[(output_channel * output_height + output_y) * output_width + output_x] = sum;
            }
        }
    }
    Ok((output_height, output_width))
}

fn flatten_conv_output(
    input: &[f32],
    channels: usize,
    mel_bins: usize,
    time: usize,
    output: &mut Vec<f32>,
) -> Result<(), String> {
    if channels == 0 || mel_bins == 0 || time == 0 {
        return Err("Convolution flatten dimensions must be non-zero".into());
    }
    let features = checked_product("convolution flattened features", channels, mel_bins)?;
    let len = checked_product("convolution flattened output", time, features)?;
    if input.len() != len || input.iter().any(|value| !value.is_finite()) {
        return Err("Invalid final convolution tensor".into());
    }
    resize_f32(output, "convolution flattened output", len)?;
    for time_index in 0..time {
        for channel in 0..channels {
            for mel in 0..mel_bins {
                let feature = channel * mel_bins + mel;
                output[time_index * features + feature] =
                    input[(channel * mel_bins + mel) * time + time_index];
            }
        }
    }
    Ok(())
}

fn apply_gelu(values: &mut [f32]) -> Result<(), String> {
    for value in values {
        let x = *value;
        let sign = if x < 0.0 { -1.0 } else { 1.0 };
        let t = 1.0 / (1.0 + 0.3275911 * (x / std::f32::consts::SQRT_2).abs());
        let erf = sign
            * (1.0
                - (((((1.0614054 * t - 1.4531521) * t) + 1.4214138) * t - 0.28449672) * t
                    + 0.2548296)
                    * t
                    * (-(x * x) / 2.0).exp());
        *value = 0.5 * x * (1.0 + erf);
        if !value.is_finite() {
            return Err("Non-finite convolution GELU output".into());
        }
    }
    Ok(())
}

fn checked_product(name: &str, left: usize, right: usize) -> Result<usize, String> {
    left.checked_mul(right)
        .ok_or_else(|| format!("{name} overflows usize"))
}

fn reserved_f32(name: &str, len: usize) -> Result<Vec<f32>, String> {
    let mut values = Vec::new();
    values
        .try_reserve_exact(len)
        .map_err(|_| format!("Failed to allocate {name}"))?;
    values.resize(len, 0.0);
    Ok(values)
}

fn resize_f32(values: &mut Vec<f32>, name: &str, len: usize) -> Result<(), String> {
    if len > values.len() {
        values
            .try_reserve_exact(len - values.len())
            .map_err(|_| format!("Failed to allocate {name}"))?;
    }
    values.resize(len, 0.0);
    Ok(())
}

fn static_tensor(
    source: &Arc<dyn TensorSource>,
    name: &str,
    dims: &[u64],
    kind: GGMLType,
) -> Result<&'static [u8], String> {
    let bytes = checked_tensor(source.as_ref(), name, dims, kind)?;
    if kind == GGMLType::F16
        && bytes
            .chunks_exact(2)
            .any(|bytes| !f16_to_f32(u16::from_le_bytes([bytes[0], bytes[1]])).is_finite())
    {
        return Err(format!("Non-finite tensor values: {name}"));
    }
    // SAFETY: Qwen3AudioModel stores a strong Arc to this immutable TensorSource before every
    // lifetime-extended slice and never exposes unloading, so the bytes live until model drop.
    Ok(unsafe { std::mem::transmute::<&[u8], &'static [u8]>(bytes) })
}

fn load_f32_tensor(
    source: &dyn TensorSource,
    name: &str,
    dims: &[u64],
) -> Result<Vec<f32>, String> {
    let bytes = checked_tensor(source, name, dims, GGMLType::F32)?;
    let values: Vec<f32> = bytes
        .chunks_exact(4)
        .map(|bytes| f32::from_le_bytes(bytes.try_into().expect("four-byte chunk")))
        .collect();
    if values.is_empty() || values.iter().any(|value| !value.is_finite()) {
        return Err(format!("Invalid finite F32 tensor: {name}"));
    }
    Ok(values)
}

fn checked_tensor<'a>(
    source: &'a dyn TensorSource,
    name: &str,
    dims: &[u64],
    kind: GGMLType,
) -> Result<&'a [u8], String> {
    let info = source
        .tensor_info(name)
        .ok_or_else(|| format!("Missing tensor: {name}"))?;
    if dims.is_empty() || dims.contains(&0) || info.dims != dims || info.ggml_type != kind {
        return Err(format!(
            "Invalid tensor {name}: shape {:?} type {:?}; expected {:?} {:?}",
            info.dims, info.ggml_type, dims, kind
        ));
    }
    let expected = usize::try_from(
        info.checked_nbytes()
            .ok_or_else(|| format!("Invalid tensor byte size: {name}"))?,
    )
    .map_err(|_| format!("Tensor byte size does not fit usize: {name}"))?;
    let bytes = source
        .tensor_slice(name)
        .ok_or_else(|| format!("Missing tensor data: {name}"))?;
    if bytes.is_empty() || bytes.len() != expected {
        return Err(format!(
            "Invalid tensor data length for {name}: {}; expected {expected}",
            bytes.len()
        ));
    }
    Ok(bytes)
}

fn to_u64(value: usize, name: &str) -> Result<u64, String> {
    u64::try_from(value).map_err(|_| format!("{name} does not fit u64"))
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
    fn conv2d_stride2_padding_and_layout_are_exact() {
        let weight_bytes: &'static [u8] = Box::leak(
            (0..9)
                .flat_map(|_| half::f16::from_f32(1.0).to_bits().to_le_bytes())
                .collect::<Vec<_>>()
                .into_boxed_slice(),
        );
        let weights = Conv2dWeights {
            weight: F16Tensor {
                bytes: weight_bytes,
                dims: vec![3, 3, 1, 1],
            },
            bias: vec![0.5],
            input_channels: 1,
            output_channels: 1,
        };
        let mut output = Vec::new();

        let (height, width) = conv2d_stride2_padding1(
            &(1..=9).map(|value| value as f32).collect::<Vec<_>>(),
            1,
            3,
            3,
            &weights,
            &mut output,
        )
        .unwrap();

        assert_eq!((height, width), (2, 2));
        assert_eq!(output, vec![12.5, 16.5, 24.5, 28.5]);
    }

    #[test]
    fn conv_output_flattens_channel_then_mel_per_time() {
        let channels = 480;
        let mel_bins = 16;
        let time = 13;
        let nchw_index = |batch: usize, channel: usize, mel: usize, time_index: usize| {
            (((batch * channels + channel) * mel_bins + mel) * time) + time_index
        };
        let source: Vec<f32> = (0..channels * mel_bins * time)
            .map(|index| index as f32)
            .collect();
        let mut flattened = Vec::new();

        flatten_conv_output(&source, channels, mel_bins, time, &mut flattened).unwrap();

        for time_index in 0..time {
            for channel in 0..channels {
                for mel in 0..mel_bins {
                    let feature = channel * 16 + mel;
                    assert_eq!(
                        flattened[time_index * 7680 + feature],
                        source[nchw_index(0, channel, mel, time_index)]
                    );
                }
            }
        }
    }

    #[test]
    fn conv2d_rejects_malformed_shapes_and_overflow() {
        let weights = Conv2dWeights {
            weight: F16Tensor {
                bytes: &[0; 18],
                dims: vec![3, 3, 1, 1],
            },
            bias: vec![0.0],
            input_channels: 1,
            output_channels: 1,
        };
        let mut output = Vec::new();

        assert!(conv2d_stride2_padding1(&[], 1, 3, 3, &weights, &mut output).is_err());
        assert!(flatten_conv_output(&[], usize::MAX, 2, 2, &mut output).is_err());
    }

    #[test]
    fn audio_linear_loader_accepts_only_fixed_q8_names() {
        assert!(is_q8_audio_linear("a.blk.0.attn_q.weight"));
        assert!(is_q8_audio_linear("a.blk.17.ffn_down.weight"));
        assert!(is_q8_audio_linear("mm.a.mlp.2.weight"));
        assert!(!is_q8_audio_linear("a.blk.18.attn_q.weight"));
        assert!(!is_q8_audio_linear("a.blk.x.attn_q.weight"));
        assert!(!is_q8_audio_linear("a.blk.0.other.weight"));
    }

    #[test]
    fn convolution_uses_erf_gelu() {
        let mut values = vec![-1.0, 0.0, 1.0, 2.0];
        apply_gelu(&mut values).unwrap();
        for (actual, expected) in values.iter().zip([-0.15865526, 0.0, 0.8413448, 1.9544997]) {
            assert!((actual - expected).abs() <= 3e-7, "{actual} != {expected}");
        }
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
    fn wav_recognizable_truncated_riff_header_is_invalid() {
        for bytes in [
            b"RIFF".as_slice(),
            b"RIFF\0\0\0\0".as_slice(),
            b"RIFF\0\0\0\0WAV".as_slice(),
        ] {
            assert!(matches!(
                decode_pcm16_wav(bytes),
                Err(AsrAudioError::Invalid(_))
            ));
        }
        assert!(matches!(
            decode_pcm16_wav(b"NOPE\0\0\0\0WAVE"),
            Err(AsrAudioError::Unsupported(_))
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

    #[test]
    fn finite_audio_power_overflow_is_invalid() {
        assert!(matches!(
            compute_log_mel(&[1.0e20]),
            Err(AsrAudioError::Invalid(_))
        ));
    }

    #[derive(Default)]
    struct MapTensorSource {
        metadata: HashMap<String, MetaValue>,
        tensors: HashMap<String, TensorInfo>,
        data: HashMap<String, Vec<u8>>,
    }

    impl TensorSource for MapTensorSource {
        fn metadata(&self, key: &str) -> Option<&MetaValue> {
            self.metadata.get(key)
        }

        fn tensor_info(&self, name: &str) -> Option<&TensorInfo> {
            self.tensors.get(name)
        }

        fn tensor_slice(&self, name: &str) -> Option<&[u8]> {
            self.data.get(name).map(Vec::as_slice)
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
            data: HashMap::new(),
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

    fn filled_f16(elements: usize, value: f32) -> Vec<u8> {
        let bytes = half::f16::from_f32(value).to_bits().to_le_bytes();
        (0..elements).flat_map(|_| bytes).collect()
    }

    fn filled_f32(elements: usize, value: f32) -> Vec<u8> {
        (0..elements).flat_map(|_| value.to_le_bytes()).collect()
    }

    #[test]
    fn zero_projection_clears_reused_result_buffer() {
        let weight: &'static [u8] = Box::leak(filled_f16(1, 2.0).into_boxed_slice());
        let linear = AudioLinear {
            weight,
            kind: GGMLType::F16,
            input: 1,
            output: 1,
            bias: Vec::new(),
        };
        let mut result = Vec::new();

        linear.project_f16(&[3.0], 1, &mut result).unwrap();
        assert_eq!(result, [6.0]);
        linear.project_f16(&[0.0], 1, &mut result).unwrap();

        assert_eq!(result, [0.0]);
    }

    #[test]
    fn one_mel_chunk_produces_thirteen_hidden_rows() {
        let mut source = valid_qwen3a_source();
        for (name, input, output) in [
            ("a.conv2d.1.weight", 1, 480),
            ("a.conv2d.2.weight", 480, 480),
            ("a.conv2d.3.weight", 480, 480),
        ] {
            source
                .data
                .insert(name.into(), filled_f16(3 * 3 * input * output, 0.001));
        }
        for name in ["a.conv2d.1.bias", "a.conv2d.2.bias", "a.conv2d.3.bias"] {
            source.data.insert(name.into(), filled_f32(480, 0.0));
        }
        source
            .data
            .insert("a.conv_out.weight".into(), filled_f16(7680 * 896, 0.001));
        let model = Qwen3AudioModel::from_source(std::sync::Arc::new(source)).unwrap();
        let window = MelWindow {
            values: vec![0.0; 128 * 100],
            frames: 100,
            valid_frames: 100,
        };

        let hidden = model.encode_convolution(&window).unwrap();

        assert_eq!(hidden.tokens, 13);
        assert_eq!(hidden.values.len(), 13 * 896);
        assert!(hidden.values.iter().all(|value| value.is_finite()));
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
