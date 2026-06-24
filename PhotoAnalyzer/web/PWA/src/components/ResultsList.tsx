import type { AnalysisResult } from "../api/photoAnalyzer";
import type { FileEntry } from "../types";

interface Props {
  results: AnalysisResult[];
  files: FileEntry[];
  onSelect: (index: number) => void;
  onExportJson: () => void;
  onExportCsv: () => void;
}

export function ResultsList({ results, files, onSelect, onExportJson, onExportCsv }: Props) {
  if (results.length === 0) {
    return (
      <div className="empty-state">
        <span className="empty-state-icon">📭</span>
        <div>还没有分析结果</div>
        <div style={{ fontSize: 13, marginTop: 8 }}>
          去「图片」标签上传照片并开始分析
        </div>
      </div>
    );
  }

  const successCount = results.filter((r) => r.success).length;

  return (
    <>
      <div className="card">
        <div className="results-summary">
          <div className="results-summary-stats">
            <div className="stat">
              <div className="stat-value">{results.length}</div>
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
                {results.length - successCount}
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
        <div className="card-header">
          <div className="card-header-icon">📷</div>
          <span>相册视图</span>
        </div>
        <div className="gallery-grid">
          {results.map((r, i) => {
            const thumb = files[i]?.thumb;
            const score = r.data?.score;
            const success = r.success;
            return (
              <div
                key={`${r.file}-${i}`}
                className={`gallery-item results-grid-item ${!success ? "has-error" : ""}`}
                onClick={() => onSelect(i)}
              >
                {thumb ? (
                  <img src={thumb} alt={r.file} loading="lazy" />
                ) : (
                  <div className="gallery-placeholder">{success ? "⏳" : "⚠️"}</div>
                )}
                {score !== undefined && success && (
                  <div className="gallery-score-badge">{score}</div>
                )}
                {!success && <div className="gallery-error-badge">失败</div>}
                <div className="gallery-item-name">{r.file}</div>
              </div>
            );
          })}
        </div>
      </div>
    </>
  );
}