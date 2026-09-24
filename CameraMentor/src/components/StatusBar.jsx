export default function StatusBar({
  status,
  statusText,
  fps,
  latency,
  running,
  onToggle,
}) {
  return (
    <div className="statusbar">
      <span className={`status-dot ${status}`} />
      <span className="status-label">{statusText}</span>
      {fps > 0 && <span className="status-meta">{fps.toFixed(1)} fps</span>}
      {latency > 0 && <span className="status-meta">{latency}ms</span>}
      <button
        className={`btn-toggle ${running ? 'running' : ''}`}
        disabled={status === 'idle' || status === 'error'}
        onClick={onToggle}
      >
        {running ? '停止' : '开始'}
      </button>
    </div>
  );
}
