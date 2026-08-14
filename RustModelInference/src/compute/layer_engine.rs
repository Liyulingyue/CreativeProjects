use super::device::{ComputeDevice, DeviceKind, Scheduler};
use super::layer::{LayerSpec, LayerScheduleConfig, LayerDeviceConfig, LayerError, LayerResult};
use std::sync::{Arc, Mutex};

pub struct GpuWeightCache {
    cached_weights: std::collections::HashMap<usize, Arc<Vec<u8>>>,
}

impl GpuWeightCache {
    pub fn new() -> Self {
        Self {
            cached_weights: std::collections::HashMap::new(),
        }
    }

    pub fn preload(&mut self, layer_id: usize, weight: Vec<u8>) {
        self.cached_weights.insert(layer_id, Arc::new(weight));
    }

    pub fn get(&self, layer_id: usize) -> Option<Arc<Vec<u8>>> {
        self.cached_weights.get(&layer_id).cloned()
    }

    pub fn is_preloaded(&self, layer_id: usize) -> bool {
        self.cached_weights.contains_key(&layer_id)
    }

    pub fn clear(&mut self) {
        self.cached_weights.clear();
    }
}

impl Default for GpuWeightCache {
    fn default() -> Self {
        Self::new()
    }
}

pub struct LayerEngine {
    scheduler: Arc<Mutex<Scheduler>>,
    weight_cache: GpuWeightCache,
    layer_config: LayerScheduleConfig,
    mode: LayerMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayerMode {
    Disabled,
    Auto,
    LayerAffinity,
}

impl LayerEngine {
    pub fn new(scheduler: Arc<Mutex<Scheduler>>) -> Self {
        Self {
            scheduler,
            weight_cache: GpuWeightCache::new(),
            layer_config: LayerScheduleConfig::new(),
            mode: LayerMode::Disabled,
        }
    }

    pub fn with_mode(mut self, mode: LayerMode) -> Self {
        self.mode = mode;
        self
    }

    pub fn set_mode(&mut self, mode: LayerMode) {
        self.mode = mode;
    }

    pub fn preload_weights(&mut self, weights: Vec<(usize, Vec<u8>)>) {
        for (layer_id, weight) in weights {
            self.weight_cache.preload(layer_id, weight);
        }
    }

    pub fn set_layer_device(&mut self, layer_id: usize, device: DeviceKind, ratio: u8) {
        if let Some(existing) = self.layer_config.layer_configs.iter_mut().find(|c| c.layer_id == layer_id) {
            existing.device = device;
            existing.ratio = ratio;
        } else {
            self.layer_config.layer_configs.push(LayerDeviceConfig {
                layer_id,
                device,
                ratio,
            });
        }
    }

    pub fn set_all_layers_device(&mut self, device: DeviceKind) {
        self.layer_config = LayerScheduleConfig::new();
        for i in 0..100 {
            self.layer_config.layer_configs.push(LayerDeviceConfig {
                layer_id: i,
                device,
                ratio: 100,
            });
        }
    }

    pub fn get_device_for_layer(&self, layer_id: usize) -> Option<DeviceKind> {
        self.layer_config.get_device_for_layer(layer_id)
    }

    pub fn get_ratio_for_layer(&self, layer_id: usize) -> u8 {
        self.layer_config.get_ratio_for_layer(layer_id)
    }

    pub fn should_use_gpu(&self, layer_id: usize) -> bool {
        match self.mode {
            LayerMode::Disabled => false,
            LayerMode::Auto => {
                self.layer_config.get_device_for_layer(layer_id)
                    .map(|d| d.is_accelerator())
                    .unwrap_or(false)
            }
            LayerMode::LayerAffinity => {
                self.layer_config.get_device_for_layer(layer_id)
                    .map(|d| matches!(d, DeviceKind::Gpu(_)))
                    .unwrap_or(false)
            }
        }
    }

    pub fn is_weight_cached(&self, layer_id: usize) -> bool {
        self.weight_cache.is_preloaded(layer_id)
    }

    pub fn get_cached_weight(&self, layer_id: usize) -> Option<Arc<Vec<u8>>> {
        self.weight_cache.get(layer_id)
    }

    pub fn clear_cache(&mut self) {
        self.weight_cache.clear();
    }
}

impl Default for LayerEngine {
    fn default() -> Self {
        Self::new(Arc::new(Mutex::new(Scheduler::new())))
    }
}
