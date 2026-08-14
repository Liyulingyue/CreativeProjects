import { MouseEvent, useRef, useState, useEffect } from "react";
import { DetectionResult, PostureResult } from "../types";
import { CameraTransportMode } from "../hooks/useCamera";

interface MonitorViewProps {
  videoRef: (node: HTMLImageElement | null) => void;
  isCameraReady: boolean;
  cameraError: string | null;
  cameraTransportMode: CameraTransportMode;
  lastDetection: DetectionResult | null;
  status: "idle" | "present" | "away" | "overworked";
  isMonitoring: boolean;
  personDetected: boolean;
  posture: PostureResult | null;
  workSecs: number;
  breakSecs: number;
  totalWork: number;
  totalBreak: number;
  workThresholdSecs: number;
  isPoseReady: boolean;
  isPoseLoading: boolean;
  poseError: string | null;
  mirrorVideo: boolean;
  onToggleMonitoring: () => void;
  onCompact: () => void;
  onSettings: () => void;
  onQuit: () => void;
  onStartWindowDrag: (event: MouseEvent<HTMLElement>) => void;
}

function formatDuration(secs: number): string {
  const total = Math.floor(secs);
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  const s = total % 60;
  if (h > 0) return `${h}:${String(m).padStart(2, "0")}:${String(s).padStart(2, "0")}`;
  return `${String(m).padStart(2, "0")}:${String(s).padStart(2, "0")}`;
}

function formatShort(secs: number): string {
  const total = Math.floor(secs);
  if (total < 60) return `${total}秒`;
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  if (h > 0) return `${h}小时${m}分`;
  return `${m}分`;
}

const STATUS_TEXT: Record<string, string> = {
  idle: "未启动",
  present: "工作中",
  away: "已离开",
  overworked: "超时",
};

function PostureChart({ history }: { history: number[] }) {
  if (history.length === 0) {
    return <div className="chart-empty">暂无数据</div>;
  }
  return (
    <div className="posture-chart">
      {history.map((score, i) => {
        const cls = score >= 70 ? "good" : score >= 50 ? "fair" : "bad";
        return (
          <div
            key={i}
            className={`chart-bar ${cls}`}
            style={{ height: `${Math.max(4, score)}%` }}
            title={`${score}分`}
          />
        );
      })}
    </div>
  );
}

function PostureCircle({ score }: { score: number }) {
  const cls = score >= 70 ? "good" : score >= 50 ? "fair" : "bad";
  const circumference = 2 * Math.PI * 26;
  const offset = circumference * (1 - score / 100);
  return (
    <div className="posture-circle">
      <svg width="64" height="64" viewBox="0 0 64 64">
        <circle className="posture-circle-bg" cx="32" cy="32" r="26" />
        <circle
          className={`posture-circle-fill ${cls}`}
          cx="32" cy="32" r="26"
          strokeDasharray={circumference}
          strokeDashoffset={offset}
        />
      </svg>
      <div className={`posture-circle-value ${cls}`}>{score}</div>
    </div>
  );
}

export default function MonitorView({
  videoRef,
  isCameraReady,
  cameraError,
  cameraTransportMode,
  lastDetection,
  status,
  isMonitoring,
  personDetected,
  posture,
  workSecs,
  breakSecs,
  totalWork,
  totalBreak,
  workThresholdSecs,
  isPoseReady,
  isPoseLoading,
  poseError,
  mirrorVideo,
  onToggleMonitoring,
  onCompact,
  onSettings,
  onQuit,
  onStartWindowDrag,
}: MonitorViewProps) {
  const [postureHistory, setPostureHistory] = useState<number[]>([]);
  const [currentTime, setCurrentTime] = useState(new Date());
  const lastSampleSeqRef = useRef<number>(-1);

  useEffect(() => {
    const t = setInterval(() => setCurrentTime(new Date()), 1000);
    return () => clearInterval(t);
  }, []);

  useEffect(() => {
    if (posture && personDetected) {
      const sampleSeq = posture.sampleSeq ?? -1;
      if (sampleSeq === lastSampleSeqRef.current) {
        return;
      }
      lastSampleSeqRef.current = sampleSeq;
      setPostureHistory(prev => {
        const next = [...prev, posture.score];
        return next.slice(-20);
      });
    }
  }, [posture?.sampleSeq, posture?.score, personDetected]);

  const safeThresholdSecs = Math.max(workThresholdSecs, 1);
  const workProgress = workSecs > 0 ? Math.min((workSecs / safeThresholdSecs) * 100, 100) : 0;
  const progressClass = workProgress < 60 ? "safe" : workProgress < 85 ? "warning" : "danger";
  const thresholdMinutes = Math.max(1, Math.round(safeThresholdSecs / 60));

  const postureScore = posture?.score ?? 0;
  const postureClass = postureScore >= 70 ? "good" : postureScore >= 50 ? "fair" : "bad";
  const postureText = postureScore >= 70 ? "坐姿良好" : postureScore >= 50 ? "坐姿一般" : "坐姿偏差";

  const timeStr = currentTime.toLocaleTimeString("zh-CN", { hour: "2-digit", minute: "2-digit" });

  return (
    <>
      <header className="app-header" onMouseDown={onStartWindowDrag}>
        <div className="app-header-left">
          <span className="app-logo">🖥️</span>
          <span className="app-header-title">WorkerMonitor</span>
        </div>
        <div className="app-header-right">
          <div className="header-status">
            <span className={`header-status-dot ${status}`} />
            {STATUS_TEXT[status]}
          </div>
          <span className="header-time">{timeStr}</span>
          <div className="header-actions">
            <button className="icon-btn" onClick={onCompact} title="紧凑模式">◻</button>
            <button className="icon-btn" onClick={onSettings} title="设置">⚙</button>
            <button className="icon-btn" onClick={onQuit} title="退出">×</button>
          </div>
        </div>
      </header>

      <div className="app-body">
        <div className="dashboard-left">
          <div className="dash-card camera-card">
            <div className="dash-card-header">
              <span className="dash-card-title">摄像头</span>
              <div style={{ display: "flex", gap: 6, alignItems: "center" }}>
                <span className={`dash-card-badge ${personDetected ? "green" : "yellow"}`}>
                  {personDetected ? "检测到人员" : "未检测"}
                </span>
                <span className={`dash-card-badge ${cameraTransportMode === "stream" ? "blue" : cameraTransportMode === "fallback-polling" ? "yellow" : "red"}`} title="摄像头预览链路">
                  {cameraTransportMode === "stream" ? "STREAM" : cameraTransportMode === "fallback-polling" ? "FALLBACK" : "IDLE"}
                </span>
              </div>
            </div>
            <div className="camera-wrapper">
              <img ref={videoRef} alt="" style={{ transform: mirrorVideo ? "scaleX(-1)" : "none" }} />
              {!isCameraReady && !cameraError && (
                <div className="camera-overlay">点击下方按钮启动监控</div>
              )}
              {cameraError && <div className="camera-overlay">{cameraError}</div>}
              {isMonitoring && (
                <div className="camera-status-bar">
                  <div className="camera-detect-indicator">
                    <span className={`camera-detect-dot ${personDetected ? "active" : ""}`} />
                    {personDetected ? "在场" : "离开"}
                  </div>
                  <span className="camera-timer-badge">
                    {status === "away" ? formatDuration(breakSecs) : formatDuration(workSecs)}
                  </span>
                </div>
              )}
              {isMonitoring && !isPoseReady && !poseError && (
                <div className="pose-loading-overlay">
                  {isPoseLoading ? "加载模型中..." : "初始化..."}
                </div>
              )}
            </div>
            <div className="last-detection" aria-live="polite">
              <div className="last-detection-header">
                <span>最近一次检测</span>
                <span className={`last-detection-state ${lastDetection?.person_detected ? "detected" : "not-detected"}`}>
                  {lastDetection ? (lastDetection.person_detected ? "检测到人员" : "未检测到人员") : "暂无结果"}
                </span>
              </div>
              {lastDetection && (
                <div className="last-detection-details">
                  <span>样本 #{lastDetection.sample_seq}</span>
                  <span>关键点 {lastDetection.keypoints.length}/17</span>
                  <span>坐姿 {lastDetection.score} 分</span>
                </div>
              )}
            </div>
          </div>

          <div className="dash-card posture-score-card">
            <div className="dash-card-header">
              <span className="dash-card-title">坐姿分析</span>
              {personDetected && posture && (
                <span className={`dash-card-badge ${postureClass === "good" ? "green" : postureClass === "fair" ? "yellow" : "red"}`}>
                  {postureText}
                </span>
              )}
            </div>
            <div className="posture-score-main">
              {personDetected && posture ? (
                <>
                  <PostureCircle score={posture.score} />
                  <div className="posture-details">
                    <div className="posture-status-text">{postureText}</div>
                    <div className="posture-issues-list">
                      {posture.headForward && <span className="posture-issue-tag">探颈</span>}
                      {posture.headTilt && <span className="posture-issue-tag">歪头</span>}
                      {posture.shoulderUneven && <span className="posture-issue-tag">肩膀不平</span>}
                      {posture.slouching && <span className="posture-issue-tag">驼背</span>}
                      {!posture.headForward && !posture.headTilt && !posture.shoulderUneven && !posture.slouching && (
                        <span className="posture-ok-tag">姿态正常</span>
                      )}
                    </div>
                  </div>
                </>
              ) : (
                <div className="chart-empty" style={{ flex: 1 }}>启动监控后显示坐姿评分</div>
              )}
            </div>
          </div>
        </div>

        <div className="dashboard-right">
          <div className="dash-card timer-card">
            <div className="dash-card-header">
              <span className="dash-card-title">工作计时</span>
              <span className={`dash-card-badge ${progressClass === "safe" ? "green" : progressClass === "warning" ? "yellow" : "red"}`}>
                {status === "away" ? "休息中" : status === "idle" ? "已暂停" : workProgress >= 85 ? "即将超时" : "进行中"}
              </span>
            </div>
            <div className="timer-main">
              <div className={`timer-value ${status}`}>
                {status === "away" ? formatDuration(breakSecs) : formatDuration(workSecs)}
              </div>
              <div className="timer-label">
                {status === "away" ? "休息时长" : status === "idle" ? "等待开始" : "连续工作"}
              </div>
            </div>
            {status !== "idle" && status !== "away" && (
              <div className="timer-progress">
                <div className="timer-progress-bar">
                  <div
                    className={`timer-progress-fill ${progressClass}`}
                    style={{ width: `${workProgress}%` }}
                  />
                </div>
                <div className="timer-progress-label">
                  <span>0</span>
                  <span>{Math.round(workProgress)}%</span>
                  <span>{thresholdMinutes}分钟</span>
                </div>
              </div>
            )}
          </div>

          <div className="dash-card">
            <div className="dash-card-header">
              <span className="dash-card-title">今日统计</span>
            </div>
            <div className="stats-grid">
              <div className="stat-box">
                <div className="stat-label">累计工作</div>
                <div className="stat-value work">{formatShort(totalWork)}</div>
              </div>
              <div className="stat-box">
                <div className="stat-label">累计休息</div>
                <div className="stat-value break">{formatShort(totalBreak)}</div>
              </div>
            </div>
          </div>

          <div className="dash-card chart-card">
            <div className="dash-card-header">
              <span className="dash-card-title">坐姿趋势</span>
            </div>
            <PostureChart history={postureHistory} />
          </div>

          <div className="dash-card controls-card">
            <div className="main-controls">
              <button
                className={`btn ${isMonitoring ? "btn-danger" : "btn-primary"}`}
                onClick={onToggleMonitoring}
                disabled={isMonitoring && !isPoseReady}
              >
                {isMonitoring ? "⏹ 停止监控" : "▶ 开始监控"}
              </button>
            </div>
          </div>
        </div>
      </div>
    </>
  );
}
