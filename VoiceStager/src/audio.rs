use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use hound::{WavSpec, WavWriter};
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};

pub struct AudioRecorder {
    stream: Option<cpal::Stream>,
    sample_buffer: Arc<Mutex<Vec<f32>>>,
    selected_device: Option<String>,
    monitor_stream: Option<cpal::Stream>,
    is_monitoring: Arc<AtomicBool>,
    level_callback: Option<Arc<Mutex<Box<dyn Fn(f32) + Send>>>>,
    actual_sample_rate: u32,
    actual_channels: u16,
}

unsafe impl Send for AudioRecorder {}
unsafe impl Sync for AudioRecorder {}

impl AudioRecorder {
    pub fn new() -> Self {
        Self {
            stream: None,
            sample_buffer: Arc::new(Mutex::new(Vec::new())),
            selected_device: None,
            monitor_stream: None,
            is_monitoring: Arc::new(AtomicBool::new(false)),
            level_callback: None,
            actual_sample_rate: 44100,
            actual_channels: 1,
        }
    }

    pub fn list_devices() -> Vec<(String, String)> {
        let host = cpal::default_host();
        let mut devices = Vec::new();
        
        if let Ok(input_devices) = host.input_devices() {
            for device in input_devices {
                if let Ok(name) = device.name() {
                    let trimmed_name = name.trim();
                    if !trimmed_name.is_empty() {
                        devices.push((trimmed_name.to_string(), trimmed_name.to_string()));
                    }
                }
            }
        }
        
        if devices.is_empty() {
            devices.push(("default".to_string(), "Default Microphone".to_string()));
        }
        
        devices
    }

    pub fn select_device(&mut self, device_id: Option<String>) {
        self.selected_device = device_id;
    }

    pub fn set_level_callback(&mut self, callback: Box<dyn Fn(f32) + Send>) {
        self.level_callback = Some(Arc::new(Mutex::new(callback)));
    }

    pub fn start_monitoring(&mut self) -> Result<(), String> {
        let device = self.get_device()?;
        let config = device
            .default_input_config()
            .map_err(|e| format!("Cannot get input config: {}", e))?;

        self.is_monitoring.store(true, Ordering::SeqCst);
        let is_active = self.is_monitoring.clone();
        let level_callback = self.level_callback.clone();

        let stream = match config.sample_format() {
            cpal::SampleFormat::F32 => device.build_input_stream(
                &config.into(),
                move |data: &[f32], _: &cpal::InputCallbackInfo| {
                    if is_active.load(Ordering::SeqCst) {
                        let level = Self::calculate_level_f32(data);
                        if let Some(cb) = &level_callback {
                            if let Ok(guard) = cb.lock() {
                                guard(level);
                            }
                        }
                    }
                },
                |err| eprintln!("Monitor stream error: {}", err),
                None,
            ),
            cpal::SampleFormat::I16 => device.build_input_stream(
                &config.into(),
                move |data: &[i16], _: &cpal::InputCallbackInfo| {
                    if is_active.load(Ordering::SeqCst) {
                        let level = Self::calculate_level_i16(data);
                        if let Some(cb) = &level_callback {
                            if let Ok(guard) = cb.lock() {
                                guard(level);
                            }
                        }
                    }
                },
                |err| eprintln!("Monitor stream error: {}", err),
                None,
            ),
            _ => return Err("Unsupported sample format".to_string()),
        }
        .map_err(|e| format!("Failed to build monitor stream: {}", e))?;

        stream.play().map_err(|e| format!("Failed to start monitor: {}", e))?;
        self.monitor_stream = Some(stream);
        Ok(())
    }

    pub fn stop_monitoring(&mut self) {
        self.is_monitoring.store(false, Ordering::SeqCst);
        if let Some(stream) = self.monitor_stream.take() {
            drop(stream);
        }
    }

    fn calculate_level_f32(data: &[f32]) -> f32 {
        if data.is_empty() {
            return 0.0;
        }
        let sum: f32 = data.iter().map(|s| s.abs()).sum();
        (sum / data.len() as f32).min(1.0)
    }

    fn calculate_level_i16(data: &[i16]) -> f32 {
        if data.is_empty() {
            return 0.0;
        }
        let sum: f32 = data.iter().map(|s| (*s as f32 / 32768.0).abs()).sum();
        (sum / data.len() as f32).min(1.0)
    }

    fn get_device(&self) -> Result<cpal::Device, String> {
        let host = cpal::default_host();
        
        if let Some(ref device_id) = self.selected_device {
            if device_id == "default" || device_id.is_empty() {
                return host.default_input_device().ok_or("No default input device".to_string());
            }
            
            if let Ok(input_devices) = host.input_devices() {
                for device in input_devices {
                    if let Ok(name) = device.name() {
                        if name == *device_id {
                            return Ok(device);
                        }
                    }
                }
            }
        }
        
        host.default_input_device().ok_or("No input device found".to_string())
    }

    pub fn start_recording(&mut self) -> Result<(), String> {
        let device = self.get_device()?;
        let config = device
            .default_input_config()
            .map_err(|e| format!("Cannot get default input config: {}", e))?;

        let channels = config.channels();
        let sample_rate = config.sample_rate().0;

        self.sample_buffer = Arc::new(Mutex::new(Vec::new()));
        let buffer = self.sample_buffer.clone();
        let recording = Arc::new(Mutex::new(true));
        let level_callback = self.level_callback.clone();
        let level_counter = Arc::new(std::sync::atomic::AtomicUsize::new(0));

        let err_fn = |err| eprintln!("Stream error: {}", err);

        let stream = match config.sample_format() {
            cpal::SampleFormat::F32 => {
                let lc = level_callback.clone();
                let lcnt = level_counter.clone();
                device.build_input_stream(
                    &config.into(),
                    move |data: &[f32], _: &cpal::InputCallbackInfo| {
                        if *recording.lock().unwrap() {
                            buffer.lock().unwrap().extend_from_slice(data);
                            let n = lcnt.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                            if n % 4 == 0 {
                                if let Some(ref cb) = lc {
                                    let level = Self::calculate_level_f32(data);
                                    if let Ok(guard) = cb.lock() { guard(level); }
                                }
                            }
                        }
                    },
                    err_fn,
                    None,
                )
            }
            cpal::SampleFormat::I16 => {
                let lc = level_callback.clone();
                let lcnt = level_counter.clone();
                device.build_input_stream(
                    &config.into(),
                    move |data: &[i16], _: &cpal::InputCallbackInfo| {
                        if *recording.lock().unwrap() {
                            let float_data: Vec<f32> = data.iter().map(|&s| s as f32 / 32768.0).collect();
                            buffer.lock().unwrap().extend_from_slice(&float_data);
                            let n = lcnt.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                            if n % 4 == 0 {
                                if let Some(ref cb) = lc {
                                    let level = Self::calculate_level_i16(data);
                                    if let Ok(guard) = cb.lock() { guard(level); }
                                }
                            }
                        }
                    },
                    err_fn,
                    None,
                )
            }
            cpal::SampleFormat::U16 => {
                let lc = level_callback.clone();
                let lcnt = level_counter.clone();
                device.build_input_stream(
                    &config.into(),
                    move |data: &[u16], _: &cpal::InputCallbackInfo| {
                        if *recording.lock().unwrap() {
                            let float_data: Vec<f32> = data.iter().map(|&s| (s as f32 / 65535.0) * 2.0 - 1.0).collect();
                            buffer.lock().unwrap().extend_from_slice(&float_data);
                            let n = lcnt.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                            if n % 4 == 0 {
                                if let Some(ref cb) = lc {
                                    let level = Self::calculate_level_f32(&float_data);
                                    if let Ok(guard) = cb.lock() { guard(level); }
                                }
                            }
                        }
                    },
                    err_fn,
                    None,
                )
            }
            _ => return Err("Unsupported sample format".to_string()),
        }
        .map_err(|e| format!("Failed to build stream: {}", e))?;

        stream.play().map_err(|e| format!("Failed to start stream: {}", e))?;
        self.stream = Some(stream);
        self.actual_sample_rate = sample_rate;
        self.actual_channels = channels;

        eprintln!(
            "[Audio] Recording started on device '{}', {} Hz, {} channels",
            device.name().unwrap_or_default(),
            sample_rate,
            channels
        );
        Ok(())
    }

    pub fn stop_recording(&mut self, output_path: &std::path::Path) -> Result<(), String> {
        if let Some(stream) = self.stream.take() {
            drop(stream);
        }

        let samples = self.sample_buffer.lock().unwrap();
        if samples.is_empty() {
            eprintln!("[Audio] No samples recorded");
            return Err("No audio recorded".to_string());
        }

        eprintln!("[Audio] Total samples: {}", samples.len());

        let target_rate = 16000u32;
        let source_len = samples.len();
        let source_num_channels = self.actual_channels as usize;
        let source_rate = self.actual_sample_rate as f32; 
        let step = (source_rate / target_rate as f32) * source_num_channels as f32;

        let spec = WavSpec {
            channels: 1,
            sample_rate: target_rate,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };

        let mut writer =
            WavWriter::create(output_path, spec).map_err(|e| format!("WAV write error: {}", e))?;

        let mut count = 0;
        let mut i = 0.0f32;
        while (i as usize) < source_len {
            let idx = i as usize;
            // 简单取第一个通道，或者可以做平均值，但 Whisper 16k 单声道通常取主通道即可
            let sample = samples[idx].clamp(-1.0, 1.0);
            let int_sample = (sample * 32767.0) as i16;
            writer
                .write_sample(int_sample)
                .map_err(|e| format!("Write sample error: {}", e))?;
            i += step;
            count += 1;
        }

        writer
            .finalize()
            .map_err(|e| format!("Finalize error: {}", e))?;

        eprintln!("[Audio] Recording saved to {:?}, {} samples", output_path, count);
        Ok(())
    }
}

impl Default for AudioRecorder {
    fn default() -> Self {
        Self::new()
    }
}
