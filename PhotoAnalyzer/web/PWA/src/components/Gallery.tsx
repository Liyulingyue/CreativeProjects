import { useRef, useCallback } from "react";
import type { FileEntry } from "../types";

interface Props {
  files: FileEntry[];
  onAdd: (entries: FileEntry[]) => void;
  onRemove: (id: string) => void;
  onClear: () => void;
  disabled?: boolean;
}

export function Gallery({ files, onAdd, onRemove, onClear, disabled }: Props) {
  const fileInputRef = useRef<HTMLInputElement>(null);
  const dropZoneRef = useRef<HTMLDivElement>(null);

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

      newEntries.forEach((entry) => {
        const reader = new FileReader();
        reader.onload = (e) => {
          const thumb = e.target?.result as string;
          entry.thumb = thumb;
        };
        reader.readAsDataURL(entry.file);
      });

      onAdd(newEntries);
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

  return (
    <div className="card">
      <div className="card-header">
        <div className="card-header-icon">📁</div>
        <span>选择待分析的照片</span>
      </div>

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
    </div>
  );
}