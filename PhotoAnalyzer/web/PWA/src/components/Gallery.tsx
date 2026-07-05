import { useRef, useCallback } from "react";
import type { RecordEntry } from "../api/storage";
import type { AnalysisLog } from "../types";

interface Props {
  records: RecordEntry[];
  onAdd: (files: File[]) => void;
  onRemove: (id: string) => void;
  onClear: () => void;
  onAnalyze: () => void;
  isAnalyzing: boolean;
  progress: { current: number; total: number };
  log: AnalysisLog[];
  disabled?: boolean;
}

const MAX_FILES = 10;

export function Gallery({
  records,
  onAdd,
  onRemove,
  onClear,
  onAnalyze,
  isAnalyzing,
  progress,
  log,
  disabled,
}: Props) {
  const fileInputRef = useRef<HTMLInputElement>(null);
  const dropZoneRef = useRef<HTMLDivElement>(null);

  const showProgress = isAnalyzing && progress.total > 0;
  const percent = progress.total > 0 ? (progress.current / progress.total) * 100 : 0;

  const processFiles = useCallback(
    (fileList: FileList | null) => {
      if (!fileList) return;
      const imageFiles = Array.from(fileList).filter((f) =>
        f.type.startsWith("image/")
      );
      if (imageFiles.length === 0) return;

      const available = MAX_FILES - records.length;
      if (available <= 0) {
        alert(`最多只能上传 ${MAX_FILES} 张图片`);
        return;
      }
      const toProcess = imageFiles.slice(0, available);
      if (imageFiles.length > available) {
        alert(`已限制为 ${available} 张（最多 ${MAX_FILES} 张）`);
      }

      onAdd(toProcess);
    },
    [onAdd, records.length]
  );

  const handleDrop = useCallback(
    (e: React.DragEvent) => {
      e.preventDefault();
      dropZoneRef.current?.classList.remove("dragover");
      processFiles(e.dataTransfer.files);
    },
    [processFiles]
  );

  const handleDragOver = useCallback((e: React.DragEvent) => {
    e.preventDefault();
    dropZoneRef.current?.classList.add("dragover");
  }, []);

  const handleDragLeave = useCallback(() => {
    dropZoneRef.current?.classList.remove("dragover");
  }, []);

  const isTouchDevice = 'ontouchstart' in window || navigator.maxTouchPoints > 0;

  return (
    <>
    <div className="card">
      <div className="card-header">
        <div className="card-header-icon">📁</div>
        <span>选择待分析的照片</span>
      </div>

      {isTouchDevice ? (
        <button
          className="add-photo-btn"
          onClick={() => fileInputRef.current?.click()}
        >
          <span>📷</span>
          <span>添加照片</span>
        </button>
      ) : (
        <div
          ref={dropZoneRef}
          className="drop-zone"
          onClick={() => fileInputRef.current?.click()}
          onDrop={handleDrop}
          onDragOver={handleDragOver}
          onDragLeave={handleDragLeave}
        >
          <div className="drop-zone-content">
            <span className="drop-zone-icon">🖼️</span>
            <div className="drop-zone-text">点击或拖拽图片到此处</div>
            <div className="drop-zone-hint">支持 JPG · PNG · WebP · GIF</div>
          </div>
        </div>
      )}

      <input
        ref={fileInputRef}
        type="file"
        multiple
        accept="image/*"
        style={{ display: "none" }}
        onChange={(e) => {
          processFiles(e.target.files);
          if (fileInputRef.current) fileInputRef.current.value = "";
        }}
      />

      {records.length > 0 && (
        <>
          <div className="gallery-toolbar">
            <span className="gallery-count">共 {records.length} 张</span>
            <button
              className="btn btn-secondary btn-compact"
              onClick={onClear}
              disabled={disabled}
            >
              清空
            </button>
          </div>

          <div className="gallery-grid">
            {records.map((record) => (
              <div
                key={record.id}
                className={`gallery-item ${record.failedAt ? "has-error" : ""}`}
              >
                {record.thumb ? (
                  <img src={record.thumb} alt={record.fileName} loading="lazy" />
                ) : (
                  <div className="gallery-placeholder">⏳</div>
                )}
                {record.failedAt && (
                  <div className="gallery-error-badge">失败</div>
                )}
                <button
                  className="gallery-remove"
                  onClick={() => onRemove(record.id)}
                  disabled={disabled}
                  aria-label="删除"
                >
                  ×
                </button>
                <div className="gallery-item-name">{record.fileName}</div>
              </div>
            ))}
          </div>
        </>
      )}

      {showProgress && (
        <div className="gallery-progress">
          <div className="progress-bar">
            <div className="progress-fill" style={{ width: `${percent}%` }} />
          </div>
          <div className="progress-text">
            正在分析 {progress.current} / {progress.total} · {Math.round(percent)}%
          </div>
        </div>
      )}

      {records.length > 0 && !showProgress && (
        <button
          className="btn btn-primary analyze-btn-inline"
          onClick={onAnalyze}
          disabled={disabled || isAnalyzing}
        >
          ✨ 分析
        </button>
      )}
    </div>

    {log.length > 0 && (
      <div className="card">
        <div className="card-header">
          <div className="card-header-icon">📋</div>
          <span>分析日志</span>
        </div>
        <div className="log-list">
          {log.map((item, i) => (
            <div
              key={i}
              className={`log-item ${item.status === "success" ? "log-success" : "log-failed"}`}
            >
              <span className="log-icon">
                {item.status === "success" ? "✅" : "❌"}
              </span>
              <span className="log-name">{item.fileName}</span>
              <span className="log-detail">
                {item.status === "success"
                  ? `评分 ${item.score}`
                  : item.error}
              </span>
            </div>
          ))}
        </div>
      </div>
    )}
    </>
  );
}