export interface MonitorSnapshot {
  status: "idle" | "present" | "away" | "overworked";
  is_monitoring: boolean;
  is_present: boolean;
  work_duration_secs: number;
  break_duration_secs: number;
  total_work_secs: number;
  total_break_secs: number;
  has_alert: boolean;
  has_posture_alert: boolean;
  work_threshold_secs: number;
  detection: DetectionResult | null;
}

export interface Keypoint {
  x: number;
  y: number;
  confidence: number;
}

export interface DetectionResult {
  person_detected: boolean;
  keypoints: Keypoint[];
  score: number;
  head_forward: boolean;
  head_tilt: boolean;
  shoulder_uneven: boolean;
  slouching: boolean;
}

export interface AppConfig {
  work_threshold_minutes: number;
  remind_interval_minutes: number;
  away_threshold_seconds: number;
  check_interval_seconds: number;
  posture_alert_threshold: number;
  auto_start_monitoring: boolean;
  notification_sound: boolean;
}

export interface PostureResult {
  score: number;
  headForward: boolean;
  headTilt: boolean;
  shoulderUneven: boolean;
  slouching: boolean;
  details: {
    headForwardAngle: number;
    headTiltAngle: number;
    shoulderDiff: number;
    slouchAngle: number;
  };
}
