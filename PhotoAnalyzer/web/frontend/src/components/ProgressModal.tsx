import { useEffect, useState } from "react";

interface ProgressModalProps {
  open: boolean;
  current: number;
  total: number;
  currentFile: string | null;
  status: "running" | "completed" | "failed" | "canceled";
  onClose: () => void;
  onCancel?: () => void;
  title?: string;
  titleRunning?: string;
  titleCompleted?: string;
  titleFailed?: string;
  titleCanceled?: string;
}

export function ProgressModal({
  open,
  current,
  total,
  currentFile,
  status,
  onClose,
  onCancel,
  title,
  titleRunning = title ?? "正在分析...",
  titleCompleted = title ?? "分析完成",
  titleFailed = title ?? "分析失败",
  titleCanceled = title ?? "分析已取消",
}: ProgressModalProps) {
  const [visible, setVisible] = useState(false);

  useEffect(() => {
    if (open) requestAnimationFrame(() => setVisible(true));
    else setVisible(false);
  }, [open]);

  if (!open) return null;

  const pct = total > 0 ? Math.round((current / total) * 100) : 0;

  const getTitle = () => {
    switch (status) {
      case "completed": return titleCompleted;
      case "failed": return titleFailed;
      case "canceled": return titleCanceled;
      default: return titleRunning;
    }
  };

  return (
    <div className={`overlay ${visible ? "overlay--visible" : ""}`}>
      <div className="progress-modal" onClick={(e) => e.stopPropagation()}>
        <h3>{getTitle()}</h3>
        <div className="progress-bar">
          <div className="progress-bar__fill" style={{ width: `${pct}%` }} />
        </div>
        <p className="progress-modal__info">
          {current} / {total} ({pct}%)
        </p>
        {currentFile && <p className="progress-modal__file">当前: {currentFile}</p>}
        {(status === "running") && onCancel && (
          <button className="btn" onClick={onCancel}>取消</button>
        )}
        {(status === "completed" || status === "failed" || status === "canceled") && (
          <button className="btn btn--primary" onClick={onClose}>确定</button>
        )}
      </div>
    </div>
  );
}
