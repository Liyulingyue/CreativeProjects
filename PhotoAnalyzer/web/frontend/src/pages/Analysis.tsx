import { useState, useEffect, useCallback } from "react";
import { useLocation } from "react-router-dom";
import { startAnalysis, startFolderAnalysis, getAnalysisJob, listResults } from "@/api/analysis";
import type { AnalysisJob, AnalysisResult } from "@/api/types";
import { AnalysisDetail } from "@/components/AnalysisDetail";
import { ProgressModal } from "@/components/ProgressModal";
import { listDirs } from "@/api/files";
import type { DirEntry } from "@/api/types";

export function Analysis() {
  const location = useLocation();
  const initialPaths = (location.state as { filePaths?: string[] })?.filePaths;

  const [dirs, setDirs] = useState<DirEntry[]>([]);
  const [selectedDir, setSelectedDir] = useState<DirEntry | null>(null);
  const [filePaths, setFilePaths] = useState<string[]>(initialPaths ?? []);
  const [results, setResults] = useState<AnalysisResult[]>([]);
  const [activeJob, setActiveJob] = useState<AnalysisJob | null>(null);
  const [detailResult, setDetailResult] = useState<AnalysisResult | null>(null);
  const [filter, setFilter] = useState<"all" | "success" | "failed">("all");
  const [loading, setLoading] = useState(false);

  useEffect(() => {
    listDirs().then(setDirs).catch(() => {});
    listResults().then(setResults).catch(() => {});
  }, []);

  useEffect(() => {
    if (initialPaths) setFilePaths(initialPaths);
  }, [initialPaths]);

  const pollJob = useCallback(async (jobId: string) => {
    try {
      const job = await getAnalysisJob(jobId);
      setActiveJob(job);
      if (job.status === "running" || job.status === "pending") {
        setTimeout(() => pollJob(jobId), 2000);
      } else {
        listResults().then(setResults).catch(() => {});
      }
    } catch {
      setTimeout(() => pollJob(jobId), 3000);
    }
  }, []);

  const handleStartFiles = async () => {
    if (filePaths.length === 0) return;
    setLoading(true);
    try {
      const job = await startAnalysis(filePaths);
      setActiveJob(job);
      pollJob(job.job_id);
    } catch (e) {
      alert(e instanceof Error ? e.message : "Failed to start analysis");
    } finally {
      setLoading(false);
    }
  };

  const handleStartFolder = async () => {
    if (!selectedDir) return;
    setLoading(true);
    try {
      const job = await startFolderAnalysis(selectedDir.id);
      setActiveJob(job);
      pollJob(job.job_id);
    } catch (e) {
      alert(e instanceof Error ? e.message : "Failed to start analysis");
    } finally {
      setLoading(false);
    }
  };

  const filteredResults = results.filter((r) => {
    if (filter === "success") return r.success;
    if (filter === "failed") return !r.success;
    return true;
  });

  const avgScore =
    results.filter((r) => r.success && r.data).length > 0
      ? Math.round(
          results
            .filter((r) => r.success && r.data)
            .reduce((sum, r) => sum + (r.data?.score ?? 0), 0) /
            results.filter((r) => r.success && r.data).length
        )
      : null;

  return (
    <div className="page">
      <h1>照片分析</h1>

      <div className="analysis-start">
        <div className="card">
          <h3>选择分析方式</h3>

          <div className="form-group">
            <label>指定文件路径</label>
            <textarea
              placeholder="每行一个文件路径&#10;例: /mnt/nas/photos/2024/IMG_001.jpg"
              value={filePaths.join("\n")}
              onChange={(e) =>
                setFilePaths(e.target.value.split("\n").filter((p) => p.trim()))
              }
              rows={4}
            />
          </div>

          <div className="form-group">
            <label>或选择目录分析</label>
            <select
              value={selectedDir?.id ?? ""}
              onChange={(e) => {
                const dir = dirs.find((d) => d.id === e.target.value);
                setSelectedDir(dir ?? null);
              }}
            >
              <option value="">-- 选择目录 --</option>
              {dirs.map((d) => (
                <option key={d.id} value={d.id}>{d.name || d.path}</option>
              ))}
            </select>
          </div>

          <div className="analysis-start__actions">
            <button
              className="btn btn--primary"
              onClick={handleStartFiles}
              disabled={loading || filePaths.length === 0}
            >
              分析指定文件
            </button>
            <button
              className="btn btn--primary"
              onClick={handleStartFolder}
              disabled={loading || !selectedDir}
            >
              分析整个目录
            </button>
          </div>
        </div>
      </div>

      <ProgressModal
        open={activeJob?.status === "running" || activeJob?.status === "pending" || false}
        current={activeJob?.progress ?? 0}
        total={activeJob?.total ?? 0}
        currentFile={activeJob?.current_file ?? null}
        status={(activeJob?.status === "running" || activeJob?.status === "pending") ? "running" : activeJob?.status ?? "running"}
        onClose={() => setActiveJob(null)}
      />

      <div className="analysis-results">
        <div className="section-header">
          <h2>分析结果</h2>
          {avgScore !== null && <span className="avg-score">平均分: {avgScore}</span>}
          <div className="filter-tabs">
            {(["all", "success", "failed"] as const).map((f) => (
              <button
                key={f}
                className={`filter-tab ${filter === f ? "filter-tab--active" : ""}`}
                onClick={() => setFilter(f)}
              >
                {f === "all" ? "全部" : f === "success" ? "成功" : "失败"} ({f === "all" ? results.length : results.filter((r) => f === "success" ? r.success : !r.success).length})
              </button>
            ))}
          </div>
        </div>

        {filteredResults.length === 0 ? (
          <div className="empty-state">暂无分析结果</div>
        ) : (
          <div className="result-list">
            {filteredResults.map((r) => (
              <div
                key={r.file_path}
                className="result-item"
                onClick={() => setDetailResult(r)}
              >
                <div className="result-item__score" data-level={r.data && r.data.score >= 70 ? "good" : r.data && r.data.score >= 40 ? "mid" : "low"}>
                  {r.success && r.data ? r.data.score : "✕"}
                </div>
                <div className="result-item__info">
                  <div className="result-item__name">{r.file_name}</div>
                  {r.success && r.data && (
                    <div className="result-item__meta">
                      {r.data.style} · {r.data.blurry} · {r.data.caption}
                    </div>
                  )}
                  {!r.success && (
                    <div className="result-item__error">{r.error}</div>
                  )}
                </div>
              </div>
            ))}
          </div>
        )}
      </div>

      {detailResult && (
        <AnalysisDetail result={detailResult} onClose={() => setDetailResult(null)} />
      )}
    </div>
  );
}
