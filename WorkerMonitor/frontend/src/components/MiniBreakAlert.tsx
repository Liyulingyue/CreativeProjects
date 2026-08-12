interface BreakAlertProps {
  workSecs: number;
  onDismiss: () => void;
}

function formatDuration(secs: number): string {
  const total = Math.floor(secs);
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  const s = total % 60;
  if (h > 0) return `${h}:${String(m).padStart(2, "0")}:${String(s).padStart(2, "0")}`;
  return `${m}:${String(s).padStart(2, "0")}`;
}

export default function MiniBreakAlert({ workSecs, onDismiss }: BreakAlertProps) {
  return (
    <div className="mini-break-alert" onClick={onDismiss}>
      <div className="mini-break-icon">🧘</div>
      <div className="mini-break-text">
        <div className="mini-break-title">休息一下</div>
        <div className="mini-break-time">{formatDuration(workSecs)}</div>
      </div>
      <button className="mini-break-btn" onClick={(e) => { e.stopPropagation(); onDismiss(); }}>
        ×
      </button>
    </div>
  );
}
