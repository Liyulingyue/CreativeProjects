import { useState, useEffect, useCallback } from "react";
import { useLocation } from "react-router-dom";
import { startAnalysis, startFolderAnalysis, getAnalysisJob, listResults } from "@/api/analysis";
import { listDirs, addDir, browseFiles } from "@/api/files";
import type { AnalysisJob, AnalysisResult, DirEntry, BrowseResult, FileNode } from "@/api/types";
import { AnalysisDetail } from "@/components/AnalysisDetail";
import { ProgressModal } from "@/components/ProgressModal";
import { PathInput } from "@/components/PathInput";
import { FolderPicker } from "@/components/FolderPicker";
import { PhotoGrid } from "@/components/PhotoGrid";

export function Analysis() {
  const location = useLocation();
  const initialPaths = (location.state as { filePaths?: string[] })?.filePaths;

  const [dirs, setDirs] = useState<DirEntry[]>([]);
  const [pathValue, setPathValue] = useState("");
  const [currentDir, setCurrentDir] = useState<DirEntry | null>(null);
  const [browse, setBrowse] = useState<BrowseResult | null>(null);
  const [selectedPaths, setSelectedPaths] = useState<Set<string>>(new Set());
  const [results, setResults] = useState<AnalysisResult[]>([]);
  const [activeJob, setActiveJob] = useState<AnalysisJob | null>(null);
  const [detailResult, setDetailResult] = useState<AnalysisResult | null>(null);
  const [filter, setFilter] = useState<"all" | "success" | "failed">("all");
  const [loading, setLoading] = useState(false);
  const [showPicker, setShowPicker] = useState(false);

  useEffect(() => {
    listDirs().then(setDirs).catch(() => {});
    listResults().then(setResults).catch(() => {});
  }, []);

  useEffect(() => {
    if (initialPaths && initialPaths.length > 0) {
      setPathValue(initialPaths[0]);
      handleBrowsePath(initialPaths[0]);
    }
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

  const handleBrowsePath = async (path: string) => {
    let dir = dirs.find((d) => d.path === path);
    if (!dir) {
      try {
        dir = await addDir(path);
        setDirs((prev) => [...prev, dir!]);
      } catch {
        return;
      }
    }
    setCurrentDir(dir);
    setSelectedPaths(new Set());
    try {
      const result = await browseFiles(dir!.id);
      setBrowse(result);
    } catch {
      setBrowse(null);
    }
  };

  const handleSelectPath = (path: string) => {
    setPathValue(path);
    handleBrowsePath(path);
  };

  const handlePickFolder = (path: string, _name: string) => {
    setPathValue(path);
    handleBrowsePath(path);
  };

  const handleNavigate = (item: FileNode) => {
    if (item.is_dir && currentDir) {
      browseFiles(currentDir.id, item.path).then(setBrowse).catch(() => {});
    }
  };

  const toggleSelect = (path: string) => {
    setSelectedPaths((prev) => {
      const next = new Set(prev);
      if (next.has(path)) next.delete(path);
      else next.add(path);
      return next;
    });
  };

  const selectAll = () => {
    if (!browse) return;
    const images = browse.items.filter((i) => !i.is_dir);
    const allSelected = images.every((i) => selectedPaths.has(i.path));
    if (allSelected) {
      setSelectedPaths(new Set());
    } else {
      setSelectedPaths(new Set(images.map((i) => i.path)));
    }
  };

  const handleStartAll = async () => {
    if (!currentDir) return;
    setLoading(true);
    try {
      const job = await startFolderAnalysis(currentDir.id);
      setActiveJob(job);
      pollJob(job.job_id);
    } catch (e) {
      alert(e instanceof Error ? e.message : "启动分析失败");
    } finally {
      setLoading(false);
    }
  };

  const handleStartSelected = async () => {
    if (selectedPaths.size === 0) return;
    setLoading(true);
    try {
      const job = await startAnalysis(Array.from(selectedPaths));
      setActiveJob(job);
      pollJob(job.job_id);
    } catch (e) {
      alert(e instanceof Error ? e.message : "启动分析失败");
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
          results.filter((r) => r.success && r.data).reduce((sum, r) => sum + (r.data?.score ?? 0), 0) /
          results.filter((r) => r.success && r.data).length
        )
      : null;

  const imageCount = browse?.items.filter((i) => !i.is_dir).length ?? 0;

  return (
    <div className="page">
      <h1>照片分析</h1>

      <div className="card">
        <h3>选择目录</h3>
        <PathInput
          value={pathValue}
          onChange={setPathValue}
          onSelect={handleSelectPath}
          onBrowse={() => setShowPicker(true)}
          placeholder="输入路径，如 /home 或 /mnt/nas"
        />
      </div>

      {browse && (
        <div className="card">
          <div className="file-browser__header">
            <div className="file-browser__info">
              <span className="file-browser__count">{imageCount} 张图片</span>
              {selectedPaths.size > 0 && (
                <span className="file-browser__selected">已选 {selectedPaths.size} 张</span>
              )}
            </div>
            <div className="file-browser__actions">
              <button className="btn btn--sm" onClick={selectAll}>
                {imageCount > 0 && browse.items.filter((i) => !i.is_dir).every((i) => selectedPaths.has(i.path))
                  ? "取消全选"
                  : "全选"}
              </button>
              <button
                className="btn btn--sm btn--primary"
                onClick={handleStartSelected}
                disabled={loading || selectedPaths.size === 0}
              >
                分析选中 ({selectedPaths.size})
              </button>
              <button
                className="btn btn--sm"
                onClick={handleStartAll}
                disabled={loading || imageCount === 0}
              >
                分析全部
              </button>
            </div>
          </div>

          {browse.items.filter((i) => i.is_dir).length > 0 && (
            <div className="folder-list">
              {browse.items.filter((i) => i.is_dir).map((item) => (
                <div key={item.path} className="folder-item" onClick={() => handleNavigate(item)}>
                  <span>📁</span>
                  <span>{item.name}</span>
                </div>
              ))}
            </div>
          )}

          <PhotoGrid
            items={browse.items}
            onSelect={handleNavigate}
            selectedPaths={selectedPaths}
            onToggleSelect={toggleSelect}
          />
        </div>
      )}

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
              <div key={r.file_path} className="result-item" onClick={() => setDetailResult(r)}>
                <div className="result-item__score" data-level={r.data && r.data.score >= 70 ? "good" : r.data && r.data.score >= 40 ? "mid" : "low"}>
                  {r.success && r.data ? r.data.score : "✕"}
                </div>
                <div className="result-item__info">
                  <div className="result-item__name">{r.file_name}</div>
                  {r.success && r.data && (
                    <div className="result-item__meta">{r.data.style} · {r.data.blurry} · {r.data.caption}</div>
                  )}
                  {!r.success && <div className="result-item__error">{r.error}</div>}
                </div>
              </div>
            ))}
          </div>
        )}
      </div>

      {detailResult && <AnalysisDetail result={detailResult} onClose={() => setDetailResult(null)} />}

      <FolderPicker open={showPicker} onClose={() => setShowPicker(false)} onSelect={handlePickFolder} />
    </div>
  );
}
