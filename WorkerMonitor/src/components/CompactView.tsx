import { PostureResult } from "../types";

interface CompactViewProps {
  status: "idle" | "present" | "away" | "overworked";
  isMonitoring: boolean;
  posture: PostureResult | null;
  personDetected: boolean;
  workSecs: number;
  breakSecs: number;
  onExpand: () => void;
  onToggleMonitoring: () => void;
  onHide: () => void;
}

function formatTimer(secs: number): string {
  const total = Math.floor(secs);
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  const s = total % 60;
  if (h > 0) return `${h}:${String(m).padStart(2, "0")}:${String(s).padStart(2, "0")}`;
  return `${String(m).padStart(2, "0")}:${String(s).padStart(2, "0")}`;
}

const STATUS_LABEL: Record<string, string> = {
  idle: "待机",
  present: "工作",
  away: "离开",
  overworked: "超时",
};

export default function CompactView({
  status,
  isMonitoring,
  posture,
  personDetected,
  workSecs,
  breakSecs,
  onExpand,
  onToggleMonitoring,
  onHide,
}: CompactViewProps) {
  const timerValue = status === "away" ? breakSecs : workSecs;
  const postureScore = posture?.score ?? -1;

  return (
    <div className="compact">
      <div className="compact-titlebar">
        <div className="compact-drag" />
        <span className="compact-brand">WM</span>
        <button className="compact-btn" onClick={onExpand} title="展开">⤢</button>
        <button className="compact-btn" onClick={onHide} title="隐藏到托盘">─</button>
      </div>

      <div className="compact-body">
        <div className="compact-status-row">
          <span className={`compact-dot ${status}`} />
          <span className="compact-status-text">{isMonitoring ? STATUS_LABEL[status] : "停止"}</span>
          <span className={`compact-timer ${status}`}>{isMonitoring ? formatTimer(timerValue) : "--:--"}</span>
        </div>

        {isMonitoring && (status === "present" || status === "overworked") && (
          <div className="compact-progress">
            <div
              className={`compact-progress-fill ${status === "overworked" ? "danger" : "safe"}`}
              style={{ width: `${Math.min((workSecs / (45 * 60)) * 100, 100)}%` }}
            />
          </div>
        )}

        <div className="compact-bottom-row">
          {isMonitoring && personDetected && postureScore >= 0 ? (
            <span className={`compact-posture ${postureScore >= 70 ? "good" : postureScore >= 50 ? "fair" : "bad"}`}>
              坐姿 {postureScore}
            </span>
          ) : (
            <span className="compact-posture none">坐姿 --</span>
          )}
          <button
            className={`compact-monitor-btn ${isMonitoring ? "active" : ""}`}
            onClick={onToggleMonitoring}
          >
            {isMonitoring ? "⏹" : "▶"}
          </button>
        </div>
      </div>
    </div>
  );
}
