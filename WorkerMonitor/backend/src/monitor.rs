use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use std::time::Instant;

use crate::config;
use crate::rtmpose::{Keypoint, PoseOutput};

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
    pub detection: Option<DetectionInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectionInfo {
    pub person_detected: bool,
    pub keypoints: Vec<Keypoint>,
    pub score: u32,
    pub head_forward: bool,
    pub head_tilt: bool,
    pub shoulder_uneven: bool,
    pub slouching: bool,
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
    detection: Option<DetectionInfo>,
}

impl MonitorInner {
    fn new(cfg: config::AppConfig) -> Self {
        Self {
            config: cfg,
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
            detection: None,
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
            detection: self.detection.clone(),
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

pub struct MonitorState(pub Mutex<MonitorInner>);

impl MonitorState {
    pub fn new() -> Self {
        let cfg = config::load_config();
        Self(Mutex::new(MonitorInner::new(cfg)))
    }

    pub fn start(&self) -> Result<(), String> {
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
        inner.detection = None;
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
            inner.detection = None;
        }
    }

    pub fn update_detection(&self, pose: PoseOutput) -> Result<(), String> {
        let mut inner = self.0.lock().map_err(|e| e.to_string())?;
        let person_detected = pose.person_detected;
        let keypoints = pose.keypoints;

        let (score, head_forward, head_tilt, shoulder_uneven, slouching) =
            if person_detected {
                let info = self.analyze_posture(&keypoints);
                (info.score, info.head_forward, info.head_tilt, info.shoulder_uneven, info.slouching)
            } else {
                (0, false, false, false, false)
            };

        inner.detection = Some(DetectionInfo {
            person_detected,
            keypoints,
            score,
            head_forward,
            head_tilt,
            shoulder_uneven,
            slouching,
        });

        inner.is_present = person_detected;
        Ok(())
    }

    fn analyze_posture(&self, keypoints: &[Keypoint]) -> DetectionInfo {
        if keypoints.len() < 17 {
            return DetectionInfo {
                person_detected: false,
                keypoints: keypoints.to_vec(),
                score: 0,
                head_forward: false,
                head_tilt: false,
                shoulder_uneven: false,
                slouching: false,
            };
        }

        let nose = &keypoints[0];
        let left_ear = &keypoints[3];
        let right_ear = &keypoints[4];
        let left_shoulder = &keypoints[5];
        let right_shoulder = &keypoints[6];
        let left_hip = &keypoints[11];
        let right_hip = &keypoints[12];

        let avg = |a: &Keypoint, b: &Keypoint| Keypoint {
            x: (a.x + b.x) / 2.0,
            y: (a.y + b.y) / 2.0,
            confidence: (a.confidence + b.confidence) / 2.0,
        };

        let ear_mid_x = (left_ear.x + right_ear.x) / 2.0;
        let shoulder_mid_x = (left_shoulder.x + right_shoulder.x) / 2.0;
        let ear_mid_y = (left_ear.y + right_ear.y) / 2.0;
        let shoulder_mid_y = (left_shoulder.y + right_shoulder.y) / 2.0;

        let dx = (ear_mid_x - shoulder_mid_x).abs();
        let dy = (ear_mid_y - shoulder_mid_y).abs();
        let head_forward_angle = if dy > 0.001 {
            (dx / dy).atan() * 180.0 / std::f32::consts::PI
        } else {
            0.0
        };
        let head_forward = head_forward_angle > 15.0;

        let shoulder_diff = (left_shoulder.y - right_shoulder.y).abs();
        let shoulder_uneven = shoulder_diff > 0.04;

        let mid_shoulder = avg(left_shoulder, right_shoulder);
        let mid_hip = avg(left_hip, right_hip);
        let slouch_dx = (mid_shoulder.x - mid_hip.x).abs();
        let slouch_dy = (mid_shoulder.y - mid_hip.y).abs();
        let slouch_angle = if slouch_dy > 0.001 {
            (slouch_dx / slouch_dy).atan() * 180.0 / std::f32::consts::PI
        } else {
            0.0
        };
        let slouching = slouch_angle > 20.0;

        let mut penalty = 0;
        if head_forward {
            penalty += 25;
        }
        if slouching {
            penalty += 30;
        }
        if shoulder_uneven {
            penalty += 15;
        }
        let score = (100 - penalty).max(0) as u32;

        DetectionInfo {
            person_detected: true,
            keypoints: keypoints.to_vec(),
            score,
            head_forward,
            head_tilt: false,
            shoulder_uneven,
            slouching,
        }
    }

    pub fn update_presence(&self, present: bool) -> Result<MonitorSnapshot, String> {
        let mut inner = self.0.lock().map_err(|e| e.to_string())?;
        if !inner.is_monitoring {
            return Ok(inner.snapshot());
        }

        let now = Instant::now();
        let was_present = inner.is_present;
        inner.is_present = present;

        if present && !was_present {
            if inner.status == MonitorStatus::Away {
                if let Some(start) = inner.break_start {
                    let break_secs = start.elapsed().as_secs_f64();
                    inner.total_break_secs += break_secs;
                    inner.break_start = None;
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
                        eprintln!(
                            "ALERT: Work threshold exceeded - {} minutes",
                            work_secs as u64 / 60
                        );
                    }
                }
            }
        }

        Ok(inner.snapshot())
    }

    pub fn report_posture(
        &self,
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
                inner.last_posture_notification = Some(Instant::now());
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
                eprintln!("POSTURE ALERT: {}", body);
            }
        } else if !is_bad {
            inner.has_posture_alert = false;
        }
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

    pub fn save_config(&self, cfg: config::AppConfig) -> Result<(), String> {
        config::save_config(&cfg)?;
        let mut inner = self.0.lock().map_err(|e| e.to_string())?;
        inner.config = cfg;
        Ok(())
    }

    pub fn dismiss_alert(&self) {
        if let Ok(mut inner) = self.0.lock() {
            inner.has_alert = false;
        }
    }
}
