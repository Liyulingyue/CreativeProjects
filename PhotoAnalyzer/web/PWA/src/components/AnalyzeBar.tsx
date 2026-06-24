interface Props {
  isAnalyzing: boolean;
  hasFiles: boolean;
  hasResults: boolean;
  progress: { current: number; total: number };
  onAnalyze: () => void;
}

export function AnalyzeBar({ isAnalyzing, hasFiles, hasResults, progress, onAnalyze }: Props) {
  const showProgress = isAnalyzing && progress.total > 0;
  const percent = progress.total > 0 ? (progress.current / progress.total) * 100 : 0;

  return (
    <div className="action-bar">
      <div className="action-bar-content">
        {showProgress ? (
          <div className="progress-container" style={{ flex: 1 }}>
            <div className="progress-bar">
              <div
                className="progress-fill"
                style={{ width: `${percent}%` }}
              />
            </div>
            <div className="progress-text">
              正在分析 {progress.current} / {progress.total} · {Math.round(percent)}%
            </div>
          </div>
        ) : (
          <button
            className="btn btn-primary"
            onClick={onAnalyze}
            disabled={!hasFiles || isAnalyzing}
          >
            ✨ {hasResults ? "重新分析" : "开始分析"}
          </button>
        )}
      </div>
    </div>
  );
}