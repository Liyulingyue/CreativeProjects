import { useEffect, useRef, useState } from "react";
import type { AnalysisResult } from "../api/photoAnalyzer";
import type { FileEntry } from "../types";

interface Props {
  results: AnalysisResult[];
  files: FileEntry[];
  initialIndex: number;
  onClose: () => void;
}

export function ImageDetail({ results, files, initialIndex, onClose }: Props) {
  const [index, setIndex] = useState(initialIndex);
  const touchStartX = useRef<number>(0);
  const touchEndX = useRef<number>(0);

  const result = results[index];
  const thumb = files[index]?.thumb;
  const fileName = files[index]?.file.name || result?.file || "";

  const goPrev = () => {
    setIndex((i) => (i > 0 ? i - 1 : results.length - 1));
  };
  const goNext = () => {
    setIndex((i) => (i < results.length - 1 ? i + 1 : 0));
  };

  useEffect(() => {
    const handleKey = (e: KeyboardEvent) => {
      if (e.key === "ArrowLeft") goPrev();
      if (e.key === "ArrowRight") goNext();
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", handleKey);
    return () => window.removeEventListener("keydown", handleKey);
  }, [results.length]);

  const handleTouchStart = (e: React.TouchEvent) => {
    touchStartX.current = e.touches[0].clientX;
  };

  const handleTouchMove = (e: React.TouchEvent) => {
    touchEndX.current = e.touches[0].clientX;
  };

  const handleTouchEnd = () => {
    const diff = touchStartX.current - touchEndX.current;
    if (Math.abs(diff) < 50) return;
    if (diff > 0) goNext();
    else goPrev();
  };

  if (!result) return null;

  const { data, error } = result;

  return (
    <div className="image-detail-overlay" onClick={(e) => {
      if (e.target === e.currentTarget) onClose();
    }}>
      <div className="image-detail">
        <div className="image-detail-header">
          <button className="image-detail-close" onClick={onClose} aria-label="关闭">
            ×
          </button>
          <div className="image-detail-counter">
            {index + 1} / {results.length}
          </div>
        </div>

        <div
          className="image-detail-content"
          onTouchStart={handleTouchStart}
          onTouchMove={handleTouchMove}
          onTouchEnd={handleTouchEnd}
        >
          {thumb && <img src={thumb} alt={fileName} className="image-detail-img" />}
        </div>

        <div className="image-detail-info">
          <div className="image-detail-filename">{fileName}</div>

          {error || !data ? (
            <div className="result-error">⚠️ {error || "分析失败"}</div>
          ) : (
            <>
              <div className="image-detail-score-row">
                <div className="image-detail-score">{data.score}</div>
                <div className="image-detail-meta">
                  <div className="meta-item">
                    <span className="meta-label">风格</span>
                    <span className="meta-value">{data.style}</span>
                  </div>
                  <div className="meta-item">
                    <span className="meta-label">清晰度</span>
                    <span className="meta-value">{data.blurry}</span>
                  </div>
                </div>
              </div>

              <div className="detail-section">
                <div className="detail-section-title">一句话描述</div>
                <div className="detail-section-content">{data.caption}</div>
              </div>

              {data.main_objects && data.main_objects.length > 0 && (
                <div className="detail-section">
                  <div className="detail-section-title">主要物体</div>
                  <div className="result-tags" style={{ padding: 0 }}>
                    {data.main_objects.map((obj, i) => (
                      <span key={i} className="result-tag">
                        {obj}
                      </span>
                    ))}
                  </div>
                </div>
              )}

              <div className="detail-section">
                <div className="detail-section-title">详细评价</div>
                <div className="detail-section-content">{data.comments}</div>
              </div>

              <div className="detail-section">
                <div className="detail-section-title">改进建议</div>
                <div className="detail-section-content">
                  {data.recommendations}
                </div>
              </div>
            </>
          )}
        </div>

        <div className="image-detail-nav">
          <button
            className="image-detail-nav-btn"
            onClick={goPrev}
            disabled={results.length <= 1}
            aria-label="上一张"
          >
            ←
          </button>
          <button
            className="image-detail-nav-btn"
            onClick={goNext}
            disabled={results.length <= 1}
            aria-label="下一张"
          >
            →
          </button>
        </div>
      </div>
    </div>
  );
}