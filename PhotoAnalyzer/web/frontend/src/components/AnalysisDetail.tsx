import { useEffect, useState } from "react";
import type { AnalysisResult } from "@/api/types";

interface AnalysisDetailProps {
  result: AnalysisResult;
  onClose: () => void;
}

export function AnalysisDetail({ result, onClose }: AnalysisDetailProps) {
  const [visible, setVisible] = useState(false);

  useEffect(() => {
    requestAnimationFrame(() => setVisible(true));
    const handler = (e: KeyboardEvent) => {
      if (e.key === "Escape") handleClose();
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, []);

  const handleClose = () => {
    setVisible(false);
    setTimeout(onClose, 200);
  };

  if (!result.success || !result.data) {
    return (
      <div className={`overlay ${visible ? "overlay--visible" : ""}`} onClick={handleClose}>
        <div className="detail-panel" onClick={(e) => e.stopPropagation()}>
          <div className="detail-panel__header">
            <h3>{result.file_name}</h3>
            <button onClick={handleClose}>✕</button>
          </div>
          <div className="detail-panel__body">
            <div className="error-msg">分析失败: {result.error}</div>
          </div>
        </div>
      </div>
    );
  }

  const { data } = result;

  return (
    <div className={`overlay ${visible ? "overlay--visible" : ""}`} onClick={handleClose}>
      <div className="detail-panel" onClick={(e) => e.stopPropagation()}>
        <div className="detail-panel__header">
          <h3>{result.file_name}</h3>
          <button onClick={handleClose}>✕</button>
        </div>
        <div className="detail-panel__body">
          <div className="detail-section">
            <div className="score-badge" data-level={data.score >= 70 ? "good" : data.score >= 40 ? "mid" : "low"}>
              {data.score}
            </div>
          </div>

          <div className="detail-section">
            <label>风格</label>
            <p>{data.style}</p>
          </div>

          <div className="detail-section">
            <label>标题</label>
            <p>{data.caption}</p>
          </div>

          <div className="detail-section">
            <label>清晰度</label>
            <p>{data.blurry}</p>
          </div>

          <div className="detail-section">
            <label>主要对象</label>
            <div className="tag-list">
              {data.main_objects.map((obj, i) => (
                <span key={i} className="tag">{obj}</span>
              ))}
            </div>
          </div>

          <div className="detail-section">
            <label>评价</label>
            <p>{data.comments}</p>
          </div>

          <div className="detail-section">
            <label>改进建议</label>
            <p>{data.recommendations}</p>
          </div>
        </div>
      </div>
    </div>
  );
}
