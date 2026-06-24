import { useRef, useCallback } from "react";
import type { FileEntry } from "../types";

interface Props {
  files: FileEntry[];
  onAdd: (entries: FileEntry[]) => void;
  onRemove: (id: string) => void;
  onClear: () => void;
  onAnalyze: () => void;
  isAnalyzing: boolean;
  hasResults: boolean;
  progress: { current: number; total: number };
  disabled?: boolean;
}

export function Gallery({
  files,
  onAdd,
  onRemove,
  onClear,
  onAnalyze,
  isAnalyzing,
  hasResults,
  progress,
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

      const newEntries: FileEntry[] = imageFiles.map((f) => ({
        id: `${Date.now()}-${Math.random().toString(36).slice(2, 9)}`,
        file: f,
      }));

      const thumbPromises = newEntries.map(
        (entry) =>
          new Promise<void>((resolve) => {
            const reader = new FileReader();
            reader.onload = (e) => {
              entry.thumb = e.target?.result as string;
              resolve();
            };
            reader.readAsDataURL(entry.file);
          })
      );

      Promise.all(thumbPromises).then(() => {
        onAdd(newEntries);
      });
    },
    [onAdd]
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

      {files.length > 0 && (
        <>
          <div className="gallery-toolbar">
            <span className="gallery-count">共 {files.length} 张</span>
            <button
              className="btn btn-secondary btn-compact"
              onClick={onClear}
              disabled={disabled}
            >
              清空
            </button>
          </div>

          <div className="gallery-grid">
            {files.map((entry) => (
              <div key={entry.id} className="gallery-item">
                {entry.thumb ? (
                  <img src={entry.thumb} alt={entry.file.name} loading="lazy" />
                ) : (
                  <div className="gallery-placeholder">⏳</div>
                )}
                <button
                  className="gallery-remove"
                  onClick={() => onRemove(entry.id)}
                  disabled={disabled}
                  aria-label="删除"
                >
                  ×
                </button>
                <div className="gallery-item-name">{entry.file.name}</div>
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

      {files.length > 0 && !showProgress && (
        <button
          className="btn btn-primary analyze-btn-inline"
          onClick={onAnalyze}
          disabled={disabled || isAnalyzing}
        >
          ✨ {hasResults ? "重新分析" : "开始分析"}
        </button>
      )}
    </div>
  );
}