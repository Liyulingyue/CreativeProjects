use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub work_threshold_minutes: u32,
    pub remind_interval_minutes: u32,
    pub away_threshold_seconds: u32,
    pub check_interval_seconds: u32,
    pub posture_alert_threshold: u32,
    pub auto_start_monitoring: bool,
    pub notification_sound: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            work_threshold_minutes: 20,
            remind_interval_minutes: 5,
            away_threshold_seconds: 90,
            check_interval_seconds: 30,
            posture_alert_threshold: 50,
            auto_start_monitoring: true,
            notification_sound: true,
        }
    }
}

fn config_dir() -> PathBuf {
    let base = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
    base.join("WorkerMonitor")
}

fn config_path() -> PathBuf {
    config_dir().join("config.json")
}

pub fn load_config() -> AppConfig {
    let path = config_path();
    if let Ok(data) = fs::read_to_string(&path) {
        if let Ok(config) = serde_json::from_str(&data) {
            return config;
        }
    }
    let config = AppConfig::default();
    let _ = save_config(&config);
    config
}

pub fn save_config(config: &AppConfig) -> Result<(), String> {
    let dir = config_dir();
    fs::create_dir_all(&dir).map_err(|e| format!("创建配置目录失败: {e}"))?;
    let data =
        serde_json::to_string_pretty(config).map_err(|e| format!("序列化配置失败: {e}"))?;
    fs::write(config_path(), data).map_err(|e| format!("写入配置失败: {e}"))?;
    Ok(())
}