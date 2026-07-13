import { useState, useEffect, useCallback, useRef } from "react";
import { useLocation } from "react-router-dom";
import { startAnalysis, startFolderAnalysis, getAnalysisJob, listResults, cancelAnalysisJob } from "@/api/analysis";
import { listDirs, addDir, browseFiles } from "@/api/files";
import type { AnalysisJob, AnalysisResult, DirEntry, BrowseResult, FileNode } from "@/api/types";
import { AnalysisDetail } from "@/components/AnalysisDetail";
import { ProgressModal } from "@/components/ProgressModal";
import { PathInput } from "@/components/PathInput";
import { FolderPicker } from "@/components/FolderPicker";
import { ImagePreview } from "@/components/ImagePreview";
import { FileBrowser } from "@/components/FileBrowser";
import { appendDirUnique, reportDuplicateDirs } from "@/utils/dirGuard";

export function Analysis() {
  const location = useLocation();
  const { 
    filePaths: initialPaths, 
    dirPath: initialDirPath,
    autoStart 
  } = (location.state as { filePaths?: string[]; dirPath?: string; autoStart?: boolean }) || {};

  const [dirs, setDirs] = useState<DirEntry[]>([]);
  const [pathValue, setPathValue] = useState("");
  const [currentDir, setCurrentDir] = useState<DirEntry | null>(null);
  const [browse, setBrowse] = useState<BrowseResult | null>(null);
  const [selectedPaths, setSelectedPaths] = useState<Set<string>>(new Set());
  const [allResults, setAllResults] = useState<AnalysisResult[]>([]);
  const [activeJob, setActiveJob] = useState<AnalysisJob | null>(null);
  const [detailResult, setDetailResult] = useState<AnalysisResult | null>(null);
  const [filter, setFilter] = useState<"all" | "success" | "failed">("all");
  const [loading, setLoading] = useState(false);
  const [showPicker, setShowPicker] = useState(false);
  const [previewItem, setPreviewItem] = useState<FileNode | null>(null);
  const [showLog, setShowLog] = useState(true);

  const hasAutoStarted = useRef(false);

  useEffect(() => {
    listDirs()
      .then((result) => {
        reportDuplicateDirs("Analysis:listDirs", result);
        setDirs(result);
      })
      .catch(() => {});
    listResults().then(setAllResults).catch(() => {});
  }, []);

  useEffect(() => {
    const preSelected = initialPaths ? new Set(initialPaths) : undefined;
    
    if (initialDirPath) {
      setPathValue(initialDirPath);
      handleBrowsePath(initialDirPath, preSelected);
    } else if (initialPaths && initialPaths.length > 0) {
      // 如果只有文件路径，尝试获取其所在目录
      const firstPath = initialPaths[0];
      const lastSlash = Math.max(firstPath.lastIndexOf("/"), firstPath.lastIndexOf("\\"));
      const folderPath = lastSlash !== -1 ? firstPath.substring(0, lastSlash) : firstPath;
      
      setPathValue(folderPath);
      handleBrowsePath(folderPath, preSelected);
    }
  }, [initialPaths, initialDirPath]);

  useEffect(() => {
    if (autoStart && initialPaths && initialPaths.length > 0 && !hasAutoStarted.current) {
      hasAutoStarted.current = true;
      handleStartSelectedInternal(initialPaths);
    }
  }, [autoStart, initialPaths]);

  const pollJob = useCallback(async (jobId: string) => {
    try {
      const job = await getAnalysisJob(jobId);
      setActiveJob(job);
      if (job.status === "running" || job.status === "pending") {
        setTimeout(() => pollJob(jobId), 2000);
      } else {
        listResults().then(setAllResults).catch(() => {});
      }
    } catch {
      setTimeout(() => pollJob(jobId), 3000);
    }
  }, []);

  const handleBrowsePath = async (path: string, preSelected?: Set<string>) => {
    let dir = dirs.find((d) => d.path === path);
    const isWindows = path.includes(":\\") || path.startsWith("\\\\");
    if (!dir) {
      // 检查路径是否已经是某个已添加目录的子目录
      const existingParent = dirs.find((d) => {
        const p = d.path;
        if (isWindows) {
          return path.toLowerCase().startsWith(p.toLowerCase()) && 
                 (path.length === p.length || path[p.length] === "\\" || path[p.length] === "/");
        }
        return path.startsWith(p) && (path.length === p.length || path[p.length] === "/");
      });

      if (existingParent) {
        dir = existingParent;
      } else {
        try {
          dir = await addDir(path);
          setDirs((prev) => {
            if (!dir) return prev;
            return appendDirUnique(prev, dir!, "Analysis:addDir");
          });
        } catch {
          return;
        }
      }
    }
    
    setCurrentDir(dir);
    setSelectedPaths(preSelected || new Set());
    
    // 如果 path 是 dir.path 的子目录，需要计算相对路径
    let subPath: string | undefined = undefined;
    if (path !== dir.path) {
      subPath = path.substring(dir.path.length).replace(/^[\\\/]+/, "");
    }

    try {
      const result = await browseFiles(dir.id, subPath);
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
    } else {
      setPreviewItem(item);
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

  const handleStartSelectedInternal = async (paths: string[]) => {
    setLoading(true);
    try {
      const job = await startAnalysis(paths);
      setActiveJob(job);
      pollJob(job.job_id);
    } catch (e) {
      alert(e instanceof Error ? e.message : "启动分析失败");
    } finally {
      setLoading(false);
    }
  };

  const handleStartSelected = () => {
    if (selectedPaths.size === 0) return;
    handleStartSelectedInternal(Array.from(selectedPaths));
  };

  const handleCancelAnalysis = async () => {
    if (!activeJob) return;
    try {
      const job = await cancelAnalysisJob(activeJob.job_id);
      setActiveJob(job);
      listResults().then(setAllResults).catch(() => {});
    } catch (e) {
      alert(e instanceof Error ? e.message : "取消分析失败");
    }
  };

  const currentResults = currentDir
    ? allResults.filter((r) => r.file_path.startsWith(currentDir.path))
    : [];

  const filteredResults = currentResults.filter((r) => {
    if (filter === "success") return r.success;
    if (filter === "failed") return !r.success;
    return true;
  });

  const avgScore =
    currentResults.filter((r) => r.success && r.data).length > 0
      ? Math.round(
          currentResults.filter((r) => r.success && r.data).reduce((sum, r) => sum + (r.data?.score ?? 0), 0) /
          currentResults.filter((r) => r.success && r.data).length
        )
      : null;

  const imageCount = browse?.items.filter((i) => !i.is_dir).length ?? 0;

  return (
    <div className="page">
      <h1>照片分析</h1>

      <div className="card">
        <h3>选择目录</h3>
        {dirs.length > 0 && (
          <div className="dir-select">
            <select
              value={currentDir?.id ?? ""}
              onChange={(e) => {
                const dir = dirs.find((d) => d.id === e.target.value);
                if (dir) {
                  setPathValue(dir.path);
                  handleBrowsePath(dir.path);
                }
              }}
            >
              <option value="">-- 选择已添加目录 --</option>
              {dirs.map((d) => (
                <option key={d.id} value={d.id}>{d.name || d.path}</option>
              ))}
            </select>
          </div>
        )}
        <PathInput
          value={pathValue}
          onChange={setPathValue}
          onSelect={handleSelectPath}
          onBrowse={() => setShowPicker(true)}
          placeholder="输入路径，如 /home 或 /mnt/nas"
        />
      </div>

      {browse && (
        <FileBrowser
          browse={browse}
          selectedPaths={selectedPaths}
          onToggleSelect={toggleSelect}
          onSelect={handleNavigate}
          onSelectAll={selectAll}
          onAction={handleStartSelected}
          onActionAll={handleStartAll}
          imageCount={imageCount}
          loading={loading}
          actionLabel="分析选中 ({n})"
          actionAllLabel="分析全部"
        />
      )}

      <ProgressModal
        open={activeJob?.status === "running" || activeJob?.status === "pending" || false}
        current={activeJob?.progress ?? 0}
        total={activeJob?.total ?? 0}
        currentFile={activeJob?.current_file ?? null}
        status={(activeJob?.status === "running" || activeJob?.status === "pending") ? "running" : activeJob?.status ?? "running"}
        onClose={() => setActiveJob(null)}
        onCancel={handleCancelAnalysis}
      />

      <div className="card card--collapsible">
        <div className="card__header" onClick={() => setShowLog((v) => !v)}>
          <div className="file-browser__info">
            <span>{showLog ? "▼" : "▶"}</span>
            <h3 style={{ margin: 0 }}>分析日志</h3>
            {currentDir && (
              <span className="file-browser__count">
                {currentResults.length} 条结果
                {avgScore !== null && ` · 平均分 ${avgScore}`}
              </span>
            )}
          </div>
          {showLog && (
            <div className="filter-tabs" onClick={(e) => e.stopPropagation()}>
              {(["all", "success", "failed"] as const).map((f) => (
                <button
                  key={f}
                  className={`filter-tab ${filter === f ? "filter-tab--active" : ""}`}
                  onClick={() => setFilter(f)}
                >
                  {f === "all" ? "全部" : f === "success" ? "成功" : "失败"} ({f === "all" ? currentResults.length : currentResults.filter((r) => f === "success" ? r.success : !r.success).length})
                </button>
              ))}
            </div>
          )}
        </div>

        {showLog && (
          <div className="analysis-log">
            {!currentDir ? (
              <div className="empty-hint">请先选择一个目录</div>
            ) : filteredResults.length === 0 ? (
              <div className="empty-hint">暂无分析结果</div>
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
        )}
      </div>

      {detailResult && <AnalysisDetail result={detailResult} onClose={() => setDetailResult(null)} />}

      <FolderPicker open={showPicker} onClose={() => setShowPicker(false)} onSelect={handlePickFolder} />

      <ImagePreview
        item={previewItem}
        onClose={() => setPreviewItem(null)}
        onAnalysisComplete={() => {
          listResults().then(setAllResults).catch(() => {});
        }}
      />
    </div>
  );
}
