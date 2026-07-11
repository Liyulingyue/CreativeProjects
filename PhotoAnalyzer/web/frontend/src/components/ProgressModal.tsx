import { useEffect, useState } from "react";

interface ProgressModalProps {
  open: boolean;
  current: number;
  total: number;
  currentFile: string | null;
  status: "running" | "completed" | "failed";
  onClose: () => void;
}

export function ProgressModal({ open, current, total, currentFile, status, onClose }: ProgressModalProps) {
  const [visible, setVisible] = useState(false);

  useEffect(() => {
    if (open) requestAnimationFrame(() => setVisible(true));
    else setVisible(false);
  }, [open]);

  if (!open) return null;

  const pct = total > 0 ? Math.round((current / total) * 100) : 0;

  return (
    <div className={`overlay ${visible ? "overlay--visible" : ""}`}>
      <div className="progress-modal" onClick={(e) => e.stopPropagation()}>
        <h3>{status === "completed" ? "分析完成" : status === "failed" ? "分析失败" : "正在分析..."}</h3>
        <div className="progress-bar">
          <div className="progress-bar__fill" style={{ width: `${pct}%` }} />
        </div>
        <p className="progress-modal__info">
          {current} / {total} ({pct}%)
        </p>
        {currentFile && <p className="progress-modal__file">当前: {currentFile}</p>}
        {status === "completed" && (
          <button className="btn btn--primary" onClick={onClose}>确定</button>
        )}
      </div>
    </div>
  );
}
