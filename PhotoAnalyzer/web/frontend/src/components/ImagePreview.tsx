import { useState, useEffect, useRef, useCallback } from "react";
import type { FileNode, AnalysisJob, AnalysisResult } from "@/api/types";
import { cancelAnalysisJob, getAnalysisJob, getResult, startAnalysis } from "@/api/analysis";
import { getThumbnailSrc } from "@/api/client";
import { ProgressModal } from "@/components/ProgressModal";

interface ImagePreviewProps {
  item: FileNode | null;
  items?: FileNode[];
  onNavigate?: (item: FileNode) => void;
  selectedPaths?: Set<string>;
  onToggleSelect?: (path: string) => void;
  onClose: () => void;
  onAnalysisComplete?: () => void;
}

export function ImagePreview({
  item,
  items,
  onNavigate,
  selectedPaths,
  onToggleSelect,
  onClose,
  onAnalysisComplete,
}: ImagePreviewProps) {
  const [result, setResult] = useState<AnalysisResult | null>(null);
  const [loading, setLoading] = useState(false);
  const [activeJob, setActiveJob] = useState<AnalysisJob | null>(null);
  const [fullSrc, setFullSrc] = useState<string>("");
  const pollTimerRef = useRef<number | null>(null);

  const clearPollTimer = useCallback(() => {
    if (pollTimerRef.current !== null) {
      window.clearTimeout(pollTimerRef.current);
      pollTimerRef.current = null;
    }
  }, []);

  const refreshResult = useCallback(async (path: string) => {
    setLoading(true);
    try {
      const data = await getResult(path);
      setResult(data);
    } catch {
      setResult(null);
    } finally {
      setLoading(false);
    }
  }, []);

  const pollJob = useCallback(
    async (jobId: string, path: string) => {
      try {
        const job = await getAnalysisJob(jobId);
        setActiveJob(job);

        if (job.status === "running" || job.status === "pending") {
          pollTimerRef.current = window.setTimeout(() => {
            void pollJob(jobId, path);
          }, 1000);
          return;
        }

        setActiveJob(null);
        await refreshResult(path);
        onAnalysisComplete?.();
      } catch {
        pollTimerRef.current = window.setTimeout(() => {
          void pollJob(jobId, path);
        }, 2000);
      }
    },
    [onAnalysisComplete, refreshResult]
  );

  useEffect(() => {
    if (!item) return;
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        onClose();
        return;
      }
      const target = e.target as HTMLElement | null;
      if (target && (target.tagName === "INPUT" || target.tagName === "TEXTAREA")) return;

      const list = items;
      if (!list || !onNavigate) return;
      const idx = list.findIndex((i) => i.path === item.path);

      if (e.key === "ArrowLeft" && idx > 0) {
        e.preventDefault();
        const prev = list[idx - 1];
        if (prev) onNavigate(prev);
      } else if (e.key === "ArrowRight" && idx >= 0 && idx < list.length - 1) {
        e.preventDefault();
        const next = list[idx + 1];
        if (next) onNavigate(next);
      } else if (e.key === " " && onToggleSelect) {
        e.preventDefault();
        onToggleSelect(item.path);
      }
    };
    document.addEventListener("keydown", handleKeyDown);
    return () => document.removeEventListener("keydown", handleKeyDown);
  }, [item, onClose, items, onNavigate, onToggleSelect]);

  useEffect(() => {
    if (!item) return;
    setResult(null);
    setActiveJob(null);
    clearPollTimer();
    setFullSrc("");
    void refreshResult(item.path);

    let active = true;
    getThumbnailSrc(item.path, true)
      .then((src) => {
        if (active) setFullSrc(src);
      })
      .catch(() => {
        if (active) setFullSrc("");
      });

    return () => {
      active = false;
    };
  }, [item, clearPollTimer, refreshResult]);

  useEffect(() => {
    return () => {
      clearPollTimer();
    };
  }, [clearPollTimer]);

  if (!item) return null;

  const analyzing = activeJob?.status === "running" || activeJob?.status === "pending";

  const list = items;
  const currentIndex = list ? list.findIndex((i) => i.path === item.path) : -1;
  const hasPrev = !!list && !!onNavigate && currentIndex > 0;
  const hasNext = !!list && !!onNavigate && currentIndex >= 0 && currentIndex < list.length - 1;
  const isSelected = !!selectedPaths && selectedPaths.has(item.path);
  const canSelect = !!onToggleSelect;
  const goPrev = () => {
    if (hasPrev && list && onNavigate) {
      const prev = list[currentIndex - 1];
      if (prev) onNavigate(prev);
    }
  };
  const goNext = () => {
    if (hasNext && list && onNavigate) {
      const next = list[currentIndex + 1];
      if (next) onNavigate(next);
    }
  };
  const toggleSelect = () => {
    if (canSelect) onToggleSelect(item.path);
  };

  const handleAnalyze = async () => {
    if (!item) return;
    clearPollTimer();
    try {
      const job = await startAnalysis([item.path]);
      setActiveJob(job);
      void pollJob(job.job_id, item.path);
    } catch {
      setActiveJob(null);
    }
  };

  const handleCancelAnalysis = async () => {
    if (!activeJob) return;
    clearPollTimer();
    try {
      const job = await cancelAnalysisJob(activeJob.job_id);
      setActiveJob(job);
      if (item) {
        await refreshResult(item.path);
      }
      onAnalysisComplete?.();
    } catch {
      setActiveJob(null);
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
          {canSelect && (
            <label className="image-preview__check" title={isSelected ? "取消勾选" : "勾选"}>
              <input
                type="checkbox"
                checked={isSelected}
                onChange={toggleSelect}
              />
            </label>
          )}
          <span className="image-preview__name" title={item.name}>{item.name}</span>
          {list && list.length > 0 && currentIndex >= 0 && (
            <span className="image-preview__counter">
              {currentIndex + 1}/{list.length}
            </span>
          )}
          {list && list.length > 0 && (
            <>
              <button
                onClick={goPrev}
                disabled={!hasPrev}
                title="上一张 (←)"
              >
                ‹
              </button>
              <button
                onClick={goNext}
                disabled={!hasNext}
                title="下一张 (→)"
              >
                ›
              </button>
            </>
          )}
          <button onClick={onClose} title="关闭 (Esc)">✕</button>
        </div>
        <div className="image-preview__body">
          <div className="image-preview__image-wrap">
            {fullSrc ? <img src={fullSrc} alt={item.name} /> : <div className="photo-card__placeholder">📷</div>}
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
      <ProgressModal
        open={activeJob?.status === "running" || activeJob?.status === "pending" || false}
        current={activeJob?.progress ?? 0}
        total={activeJob?.total ?? 0}
        currentFile={activeJob?.current_file ?? null}
        status={(activeJob?.status === "running" || activeJob?.status === "pending") ? "running" : activeJob?.status ?? "running"}
        onClose={() => setActiveJob(null)}
        onCancel={handleCancelAnalysis}
      />
    </div>
  );
}

function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}
