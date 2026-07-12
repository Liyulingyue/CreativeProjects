import { useState, useEffect } from "react";
import type { FileNode, AnalysisResult } from "@/api/types";
import { startAnalysis } from "@/api/analysis";

interface ImagePreviewProps {
  item: FileNode | null;
  onClose: () => void;
  onAnalysisComplete?: () => void;
}

export function ImagePreview({ item, onClose, onAnalysisComplete }: ImagePreviewProps) {
  const [result, setResult] = useState<AnalysisResult | null>(null);
  const [loading, setLoading] = useState(false);
  const [analyzing, setAnalyzing] = useState(false);

  useEffect(() => {
    if (!item) return;
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    document.addEventListener("keydown", handleKeyDown);
    return () => document.removeEventListener("keydown", handleKeyDown);
  }, [item, onClose]);

  useEffect(() => {
    if (!item) return;
    setLoading(true);
    setResult(null);
    fetch(`/api/results/${encodeURIComponent(item.path)}`)
      .then((res) => {
        if (!res.ok) return null;
        return res.json();
      })
      .then((data) => setResult(data))
      .catch(() => setResult(null))
      .finally(() => setLoading(false));
  }, [item]);

  if (!item) return null;

  const fullUrl = `/api/thumbnails?path=${encodeURIComponent(item.path)}&full=1`;

  const handleAnalyze = async () => {
    if (!item) return;
    setAnalyzing(true);
    try {
      const job = await startAnalysis([item.path]);
      const poll = setInterval(async () => {
        const res = await fetch(`/api/analysis/${job.job_id}`);
        const data = await res.json();
        if (data.status === "completed" || data.status === "failed") {
          clearInterval(poll);
          setAnalyzing(false);
          fetch(`/api/results/${encodeURIComponent(item.path)}`)
            .then((res) => res.ok ? res.json() : null)
            .then((r) => setResult(r))
            .catch(() => {});
          onAnalysisComplete?.();
        }
      }, 1000);
    } catch {
      setAnalyzing(false);
    }
  };

  const scoreColor = result?.data
    ? result.data.score >= 70
      ? "score--good"
      : result.data.score >= 40
      ? "score--mid"
      : "score--low"
    : "";

  return (
    <div className="overlay overlay--visible" onClick={onClose}>
      <div className="image-preview" onClick={(e) => e.stopPropagation()}>
        <div className="image-preview__header">
          <span className="image-preview__name" title={item.name}>{item.name}</span>
          <button onClick={onClose}>✕</button>
        </div>
        <div className="image-preview__body">
          <div className="image-preview__image-wrap">
            <img src={fullUrl} alt={item.name} />
          </div>
          {result || loading || analyzing ? (
            <div className="image-preview__sidebar">
              {loading ? (
                <div className="image-preview__loading">加载评分中...</div>
              ) : analyzing ? (
                <div className="image-preview__loading">分析中...</div>
              ) : result?.success && result.data ? (
                <>
                  <div className={`image-preview__score ${scoreColor}`}>
                    {result.data.score}
                  </div>
                  <div className="image-preview__meta">
                    <div className="image-preview__meta-row">
                      <span className="image-preview__meta-label">风格</span>
                      <span>{result.data.style}</span>
                    </div>
                    <div className="image-preview__meta-row">
                      <span className="image-preview__meta-label">清晰度</span>
                      <span>{result.data.blurry}</span>
                    </div>
                    <div className="image-preview__meta-row">
                      <span className="image-preview__meta-label">描述</span>
                      <span>{result.data.caption}</span>
                    </div>
                  </div>
                  {result.data.comments && (
                    <div className="image-preview__comments">{result.data.comments}</div>
                  )}
                </>
              ) : result?.error ? (
                <div className="image-preview__error">
                  <div className="image-preview__error-title">分析失败</div>
                  <div className="image-preview__error-msg">{result.error}</div>
                  <button className="btn btn--sm" onClick={handleAnalyze}>重新分析</button>
                </div>
              ) : (
                <div className="image-preview__unrated">
                  <div className="image-preview__unrated-icon">?</div>
                  <div className="image-preview__unrated-text">未评分</div>
                  <button className="btn btn--sm btn--primary" onClick={handleAnalyze}>开始评分</button>
                </div>
              )}
            </div>
          ) : (
            <div className="image-preview__sidebar">
              <div className="image-preview__unrated">
                <div className="image-preview__unrated-icon">?</div>
                <div className="image-preview__unrated-text">未评分</div>
                <button className="btn btn--sm btn--primary" onClick={handleAnalyze}>开始评分</button>
              </div>
            </div>
          )}
        </div>
        <div className="image-preview__footer">
          <span>{item.name}</span>
          <span>{item.size ? formatSize(item.size) : ""}</span>
        </div>
      </div>
    </div>
  );
}

function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}
