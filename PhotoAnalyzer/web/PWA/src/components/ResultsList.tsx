import type { RecordEntry } from "../api/storage";

interface Props {
  records: RecordEntry[];
  onSelect: (index: number) => void;
  onExportJson: () => void;
  onExportCsv: () => void;
  onClear: () => void;
}

export function ResultsList({ records, onSelect, onExportJson, onExportCsv, onClear }: Props) {
  if (records.length === 0) {
    return (
      <div className="empty-state">
        <span className="empty-state-icon">📭</span>
        <div>还没有分析结果</div>
        <div style={{ fontSize: 13, marginTop: 8 }}>
          去「分析」标签上传照片并开始分析
        </div>
      </div>
    );
  }

  const successCount = records.filter((r) => r.result?.success).length;
  const failCount = records.length - successCount;

  const sorted = [...records].sort(
    (a, b) => (b.analyzedAt || 0) - (a.analyzedAt || 0)
  );

  return (
    <>
      <div className="card">
        <div className="results-summary">
          <div className="results-summary-stats">
            <div className="stat">
              <div className="stat-value">{records.length}</div>
              <div className="stat-label">总计</div>
            </div>
            <div className="stat">
              <div className="stat-value" style={{ color: "var(--success)" }}>
                {successCount}
              </div>
              <div className="stat-label">成功</div>
            </div>
            <div className="stat">
              <div className="stat-value" style={{ color: "var(--error)" }}>
                {failCount}
              </div>
              <div className="stat-label">失败</div>
            </div>
          </div>
          <div style={{ display: "flex", gap: 8 }}>
            <button className="btn btn-secondary btn-compact" onClick={onExportJson}>
              📄 JSON
            </button>
            <button className="btn btn-secondary btn-compact" onClick={onExportCsv}>
              📊 CSV
            </button>
          </div>
        </div>
      </div>

      <div className="card">
        <div className="card-header" style={{ justifyContent: "space-between" }}>
          <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
            <div className="card-header-icon">📷</div>
            <span>分析历史</span>
          </div>
          <button className="btn btn-secondary btn-compact" onClick={onClear}>
            🗑️ 清空
          </button>
        </div>
        <div className="history-list">
          {sorted.map((record) => {
            const score = record.result?.data?.score;
            const success = record.result?.success;
            const caption = record.result?.data?.caption;
            return (
              <div
                key={record.id}
                className={`history-item ${!success ? "has-error" : ""}`}
                onClick={() => onSelect(records.indexOf(record))}
              >
                {record.thumb ? (
                  <img src={record.thumb} alt={record.fileName} className="history-thumb" />
                ) : (
                  <div className="gallery-placeholder">⏳</div>
                )}
                <div className="history-info">
                  <div className="history-filename">{record.fileName}</div>
                  <div className="history-caption">{caption || (success ? "分析成功" : record.result?.error || "分析失败")}</div>
                </div>
                {score !== undefined && success && (
                  <div className="history-score">{score}</div>
                )}
                {!success && <div className="history-error-badge">失败</div>}
              </div>
            );
          })}
        </div>
      </div>
    </>
  );
}