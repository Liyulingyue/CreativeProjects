import { RefObject } from "react";
import { PostureResult } from "../types";

interface MonitorViewProps {
  videoRef: RefObject<HTMLVideoElement | null>;
  canvasRef: RefObject<HTMLCanvasElement | null>;
  isCameraReady: boolean;
  cameraError: string | null;
  status: "idle" | "present" | "away" | "overworked";
  isMonitoring: boolean;
  personDetected: boolean;
  posture: PostureResult | null;
  workSecs: number;
  breakSecs: number;
  totalWork: number;
  totalBreak: number;
  isPoseReady: boolean;
  isPoseLoading: boolean;
  poseError: string | null;
  onToggleMonitoring: () => void;
}

function formatDuration(secs: number): string {
  const total = Math.floor(secs);
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  const s = total % 60;
  if (h > 0) return `${h}:${String(m).padStart(2, "0")}:${String(s).padStart(2, "0")}`;
  return `${String(m).padStart(2, "0")}:${String(s).padStart(2, "0")}`;
}

const STATUS_TEXT: Record<string, string> = {
  idle: "未启动",
  present: "工作中",
  away: "已离开",
  overworked: "超时工作",
};

export default function MonitorView({
  videoRef,
  canvasRef,
  isCameraReady,
  cameraError,
  status,
  isMonitoring,
  personDetected,
  posture,
  workSecs,
  breakSecs,
  totalWork,
  totalBreak,
  isPoseReady,
  isPoseLoading,
  poseError,
  onToggleMonitoring,
}: MonitorViewProps) {
  const cameraClass = isMonitoring
    ? status === "overworked"
      ? "overwork"
      : personDetected
        ? "detecting"
        : "no-detect"
    : "";

  const workProgress = workSecs > 0 ? Math.min((workSecs / (45 * 60)) * 100, 100) : 0;
  const progressClass = workProgress < 60 ? "safe" : workProgress < 85 ? "warning" : "danger";

  const postureScore = posture?.score ?? 100;
  const postureClass = postureScore >= 70 ? "good" : postureScore >= 50 ? "fair" : "bad";

  return (
    <div className="monitor-view">
      <div className={`camera-container ${cameraClass}`}>
        <video ref={videoRef} playsInline muted />
        <canvas ref={canvasRef} style={{ display: "none" }} />
        {!isCameraReady && !cameraError && (
          <div className="camera-placeholder">点击下方按钮启动监控</div>
        )}
        {cameraError && <div className="camera-placeholder">摄像头错误</div>}
        {isMonitoring && (
          <div className={`motion-indicator ${personDetected ? "active" : ""}`} />
        )}
        {isMonitoring && !isPoseReady && !poseError && (
          <div className="pose-loading">
            {isPoseLoading ? "加载 Pose 模型..." : "等待模型初始化"}
          </div>
        )}
      </div>

      {cameraError && <div className="camera-error">{cameraError}</div>}
      {poseError && (
        <div className="camera-error">Pose 模型加载失败: {poseError}</div>
      )}

      <div className="status-section">
        <div className="status-card">
          <div className="status-row">
            <span className="status-label">状态</span>
            <span className={`status-badge ${status}`}>
              <span className={`status-dot ${status}`} />
              {STATUS_TEXT[status]}
            </span>
          </div>

          <div className={`timer-display ${status}`}>
            {status === "away" ? formatDuration(breakSecs) : formatDuration(workSecs)}
          </div>
          <div className="timer-label">
            {status === "away" ? "离开时长" : status === "idle" ? "等待启动" : "连续工作时长"}
          </div>

          {(status === "present" || status === "overworked") && (
            <div className="progress-bar-container">
              <div
                className={`progress-bar ${progressClass}`}
                style={{ width: `${workProgress}%` }}
              />
            </div>
          )}

          <div className="status-row" style={{ marginTop: 12 }}>
            <span className="status-label">累计工作</span>
            <span className="status-value">{formatDuration(totalWork + workSecs)}</span>
          </div>
          <div className="status-row">
            <span className="status-label">累计休息</span>
            <span className="status-value">{formatDuration(totalBreak + breakSecs)}</span>
          </div>
        </div>

        {posture && personDetected && (
          <div className="posture-card">
            <div className="status-row">
              <span className="status-label">坐姿评分</span>
              <span className={`posture-score ${postureClass}`}>{posture.score}</span>
            </div>
            <div className="posture-bar-container">
              <div
                className={`posture-bar ${postureClass}`}
                style={{ width: `${posture.score}%` }}
              />
            </div>
            <div className="posture-issues">
              {posture.headForward && <span className="posture-issue">探颈</span>}
              {posture.headTilt && <span className="posture-issue">歪头</span>}
              {posture.shoulderUneven && <span className="posture-issue">肩膀不平</span>}
              {posture.slouching && <span className="posture-issue">含胸驼背</span>}
              {!posture.headForward && !posture.headTilt && !posture.shoulderUneven && !posture.slouching && (
                <span className="posture-ok">坐姿良好</span>
              )}
            </div>
          </div>
        )}
      </div>

      <div className="controls">
        <button
          className={`btn ${isMonitoring ? "btn-danger" : "btn-primary"}`}
          onClick={onToggleMonitoring}
          disabled={isMonitoring && !isPoseReady}
        >
          {isMonitoring ? "停止监控" : "开始监控"}
        </button>
      </div>
    </div>
  );
}
