use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use std::time::Instant;
use tauri::{AppHandle, Emitter};
use tauri_plugin_notification::NotificationExt;

use crate::config;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum MonitorStatus {
    Idle,
    Present,
    Away,
    Overworked,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitorSnapshot {
    pub status: MonitorStatus,
    pub is_monitoring: bool,
    pub is_present: bool,
    pub work_duration_secs: f64,
    pub break_duration_secs: f64,
    pub total_work_secs: f64,
    pub total_break_secs: f64,
    pub has_alert: bool,
    pub has_posture_alert: bool,
    pub work_threshold_secs: f64,
}

struct MonitorInner {
    config: config::AppConfig,
    status: MonitorStatus,
    is_present: bool,
    is_monitoring: bool,
    work_start: Option<Instant>,
    break_start: Option<Instant>,
    last_detected: Option<Instant>,
    total_work_secs: f64,
    total_break_secs: f64,
    has_alert: bool,
    has_posture_alert: bool,
    last_notification: Option<Instant>,
    last_posture_notification: Option<Instant>,
}

impl MonitorInner {
    fn new(config: config::AppConfig) -> Self {
        Self {
            config,
            status: MonitorStatus::Idle,
            is_present: false,
            is_monitoring: false,
            work_start: None,
            break_start: None,
            last_detected: None,
            total_work_secs: 0.0,
            total_break_secs: 0.0,
            has_alert: false,
            has_posture_alert: false,
            last_notification: None,
            last_posture_notification: None,
        }
    }

    fn snapshot(&self) -> MonitorSnapshot {
        let work_duration_secs = self.current_work_secs();
        let break_duration_secs = self.current_break_secs();

        MonitorSnapshot {
            status: self.status.clone(),
            is_monitoring: self.is_monitoring,
            is_present: self.is_present,
            work_duration_secs,
            break_duration_secs,
            total_work_secs: self.total_work_secs + work_duration_secs,
            total_break_secs: self.total_break_secs + break_duration_secs,
            has_alert: self.has_alert,
            has_posture_alert: self.has_posture_alert,
            work_threshold_secs: self.config.work_threshold_minutes as f64 * 60.0,
        }
    }

    fn current_work_secs(&self) -> f64 {
        if self.status == MonitorStatus::Present || self.status == MonitorStatus::Overworked {
            if let Some(start) = self.work_start {
                return start.elapsed().as_secs_f64();
            }
        }
        0.0
    }

    fn current_break_secs(&self) -> f64 {
        if self.status == MonitorStatus::Away {
            if let Some(start) = self.break_start {
                return start.elapsed().as_secs_f64();
            }
        }
        0.0
    }
}

#[allow(private_interfaces)]
pub struct MonitorState(pub(crate) Mutex<MonitorInner>);

impl MonitorState {
    pub fn new() -> Self {
        let config = config::load_config();
        Self(Mutex::new(MonitorInner::new(config)))
    }

    pub fn start(&self, app: AppHandle) -> Result<(), String> {
        let mut inner = self.0.lock().map_err(|e| e.to_string())?;
        if inner.is_monitoring {
            return Ok(());
        }
        inner.is_monitoring = true;
        inner.status = MonitorStatus::Idle;
        inner.is_present = false;
        inner.work_start = None;
        inner.break_start = None;
        inner.last_detected = None;
        inner.total_work_secs = 0.0;
        inner.total_break_secs = 0.0;
        inner.has_alert = false;
        inner.has_posture_alert = false;
        inner.last_notification = None;
        inner.last_posture_notification = None;
        drop(inner);

        let _ = app.emit("monitor-status-changed", ());
        Ok(())
    }

    pub fn stop(&self) {
        if let Ok(mut inner) = self.0.lock() {
            if let Some(start) = inner.work_start {
                inner.total_work_secs += start.elapsed().as_secs_f64();
            }
            if let Some(start) = inner.break_start {
                inner.total_break_secs += start.elapsed().as_secs_f64();
            }
            inner.is_monitoring = false;
            inner.status = MonitorStatus::Idle;
            inner.is_present = false;
            inner.work_start = None;
            inner.break_start = None;
            inner.has_alert = false;
            inner.has_posture_alert = false;
        }
    }

    pub fn update_presence(
        &self,
        present: bool,
        app: AppHandle,
    ) -> Result<MonitorSnapshot, String> {
        let mut inner = self.0.lock().map_err(|e| e.to_string())?;
        if !inner.is_monitoring {
            return Ok(inner.snapshot());
        }

        let now = Instant::now();
        let was_present = inner.is_present;
        inner.is_present = present;
        inner.last_detected = if present { Some(now) } else { inner.last_detected };

        if present && !was_present {
            if inner.status == MonitorStatus::Away {
                if let Some(start) = inner.break_start {
                    let break_secs = start.elapsed().as_secs_f64();
                    inner.total_break_secs += break_secs;
                    inner.break_start = None;
                    drop(inner);
                    let _ = app.emit("break-ended", break_secs);
                    inner = self.0.lock().map_err(|e| e.to_string())?;
                }
            }
            if inner.work_start.is_none() {
                inner.work_start = Some(now);
            }
            if inner.status == MonitorStatus::Overworked {
                inner.has_alert = false;
            }
            inner.status = MonitorStatus::Present;
        } else if !present && was_present {
            if let Some(start) = inner.work_start {
                let work_secs = start.elapsed().as_secs_f64();
                inner.total_work_secs += work_secs;
                inner.work_start = None;
            }
            inner.break_start = Some(now);
            inner.has_alert = false;
            inner.has_posture_alert = false;
            inner.status = MonitorStatus::Away;
        }

        if present {
            if let Some(start) = inner.work_start {
                let work_secs = start.elapsed().as_secs_f64();
                let threshold_secs = inner.config.work_threshold_minutes as f64 * 60.0;
                if work_secs >= threshold_secs && inner.status != MonitorStatus::Overworked {
                    inner.status = MonitorStatus::Overworked;
                    inner.has_alert = true;
                    let should_notify = match inner.last_notification {
                        None => true,
                        Some(last) => last.elapsed().as_secs() >= 300,
                    };
                    if should_notify {
                        inner.last_notification = Some(now);
                        drop(inner);
                        let _ = app.emit("work-threshold-exceeded", work_secs);
                        let _ = app.notification()
                            .builder()
                            .title("该休息了！")
                            .body(&format!("你已经连续工作了 {} 分钟，起来活动一下吧！", work_secs as u64 / 60))
                            .show();
                        inner = self.0.lock().map_err(|e| e.to_string())?;
                    }
                }

                let remind_interval_secs = inner.config.remind_interval_minutes as f64 * 60.0;
                if inner.status == MonitorStatus::Overworked && remind_interval_secs > 0.0 {
                    if let Some(last) = inner.last_notification {
                        if last.elapsed().as_secs_f64() >= remind_interval_secs {
                            inner.last_notification = Some(now);
                            drop(inner);
                            let _ = app.emit("work-threshold-exceeded", work_secs);
                            let _ = app.notification()
                                .builder()
                                .title("还在工作？")
                                .body(&format!("你已经连续工作 {} 分钟了，请尽快休息！", work_secs as u64 / 60))
                                .show();
                            inner = self.0.lock().map_err(|e| e.to_string())?;
                        }
                    }
                }
            }
        }

        let snap = inner.snapshot();
        drop(inner);

        let _ = app.emit("monitor-status-changed", ());
        crate::update_tray_tooltip(&app, &snap);

        Ok(snap)
    }

    pub fn report_posture(
        &self,
        app: AppHandle,
        score: u32,
        head_forward: bool,
        head_tilt: bool,
        shoulder_uneven: bool,
        slouching: bool,
    ) -> Result<(), String> {
        let mut inner = self.0.lock().map_err(|e| e.to_string())?;
        if !inner.is_monitoring || !inner.is_present {
            return Ok(());
        }

        let threshold = inner.config.posture_alert_threshold;
        let is_bad = score < threshold;

        if is_bad && !inner.has_posture_alert {
            inner.has_posture_alert = true;
            let should_notify = match inner.last_posture_notification {
                None => true,
                Some(last) => last.elapsed().as_secs() >= 120,
            };
            if should_notify {
                let now = Instant::now();
                inner.last_posture_notification = Some(now);
                let mut issues = Vec::new();
                if head_forward {
                    issues.push("探颈");
                }
                if head_tilt {
                    issues.push("歪头");
                }
                if shoulder_uneven {
                    issues.push("肩膀不平");
                }
                if slouching {
                    issues.push("含胸驼背");
                }
                let body = if issues.is_empty() {
                    "坐姿评分较低，请注意调整坐姿".to_string()
                } else {
                    format!("检测到{}，请注意调整坐姿", issues.join("、"))
                };
                drop(inner);
                let _ = app.emit("posture-alert", score);
                let _ = app.notification()
                    .builder()
                    .title("注意坐姿！")
                    .body(&body)
                    .show();
                inner = self.0.lock().map_err(|e| e.to_string())?;
            }
        } else if !is_bad {
            inner.has_posture_alert = false;
        }

        drop(inner);
        Ok(())
    }

    pub fn snapshot(&self) -> Result<MonitorSnapshot, String> {
        let inner = self.0.lock().map_err(|e| e.to_string())?;
        Ok(inner.snapshot())
    }

    pub fn get_config(&self) -> Result<config::AppConfig, String> {
        let inner = self.0.lock().map_err(|e| e.to_string())?;
        Ok(inner.config.clone())
    }

    pub fn save_config(&self, config: config::AppConfig, app: AppHandle) -> Result<(), String> {
        config::save_config(&config)?;
        let mut inner = self.0.lock().map_err(|e| e.to_string())?;
        inner.config = config;
        drop(inner);
        let _ = app.emit("config-changed", ());
        Ok(())
    }

    pub fn dismiss_alert(&self) {
        if let Ok(mut inner) = self.0.lock() {
            inner.has_alert = false;
        }
    }
}
