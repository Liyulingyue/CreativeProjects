import { useState, useEffect, useCallback } from "react";
import { startDedupFolder, startDedupPaths, getDedupJob, resolveDedupGroups } from "@/api/dedup";
import { listDirs, addDir, browseFiles } from "@/api/files";
import type { DirEntry, DedupJob, DedupGroup, BrowseResult, FileNode } from "@/api/types";
import { apiUrl } from "@/api/client";
import { PathInput } from "@/components/PathInput";
import { FolderPicker } from "@/components/FolderPicker";
import { PhotoGrid } from "@/components/PhotoGrid";

export function Dedup() {
  const [dirs, setDirs] = useState<DirEntry[]>([]);
  const [pathValue, setPathValue] = useState("");
  const [currentDir, setCurrentDir] = useState<DirEntry | null>(null);
  const [browse, setBrowse] = useState<BrowseResult | null>(null);
  const [selectedPaths, setSelectedPaths] = useState<Set<string>>(new Set());
  const [job, setJob] = useState<DedupJob | null>(null);
  const [expandedGroups, setExpandedGroups] = useState<Set<string>>(new Set());
  const [keepSelections, setKeepSelections] = useState<Record<string, string>>({});
  const [loading, setLoading] = useState(false);
  const [showPicker, setShowPicker] = useState(false);

  useEffect(() => {
    listDirs().then(setDirs).catch(() => {});
  }, []);

  const pollJob = useCallback(async (jobId: string) => {
    try {
      const result = await getDedupJob(jobId);
      setJob(result);
      if (result.status === "running" || result.status === "pending") {
        setTimeout(() => pollJob(jobId), 2000);
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
      const result = await startDedupFolder(currentDir.id);
      setJob(result);
      pollJob(result.job_id);
    } catch (e) {
      alert(e instanceof Error ? e.message : "启动去重失败");
    } finally {
      setLoading(false);
    }
  };

  const handleStartSelected = async () => {
    if (selectedPaths.size === 0) return;
    setLoading(true);
    try {
      const result = await startDedupPaths(Array.from(selectedPaths));
      setJob(result);
      pollJob(result.job_id);
    } catch (e) {
      alert(e instanceof Error ? e.message : "启动去重失败");
    } finally {
      setLoading(false);
    }
  };

  const toggleGroup = (groupId: string) => {
    setExpandedGroups((prev) => {
      const next = new Set(prev);
      if (next.has(groupId)) next.delete(groupId);
      else next.add(groupId);
      return next;
    });
  };

  const selectKeep = (groupId: string, path: string) => {
    setKeepSelections((prev) => ({ ...prev, [groupId]: path }));
  };

  const handleResolve = async () => {
    if (!job) return;
    const actions = Object.entries(keepSelections).map(([groupId, keep]) => {
      const group = job.groups.find((g) => g.group_id === groupId);
      return {
        group_id: groupId,
        keep,
        remove: group?.items.filter((i) => i.path !== keep).map((i) => i.path) ?? [],
      };
    });

    try {
      await resolveDedupGroups(job.job_id, actions);
      setJob(null);
      setKeepSelections({});
      setExpandedGroups(new Set());
    } catch (e) {
      alert(e instanceof Error ? e.message : "Failed to resolve");
    }
  };

  const imageCount = browse?.items.filter((i) => !i.is_dir).length ?? 0;

  return (
    <div className="page">
      <h1>照片去重</h1>

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
                去重选中 ({selectedPaths.size})
              </button>
              <button
                className="btn btn--sm"
                onClick={handleStartAll}
                disabled={loading || imageCount === 0}
              >
                去重全部
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

      {job && job.status === "running" && (
        <div className="card dedup-progress">
          <p>正在去重分析...</p>
          <p>阶段: {job.stage || "—"}</p>
          <p>文件数: {job.total_files}</p>
        </div>
      )}

      {job && (job.status === "completed" || job.status === "running") && job.groups.length > 0 && (
        <div className="dedup-results">
          <div className="section-header">
            <h2>重复组 ({job.groups.length})</h2>
            {job.status === "completed" && Object.keys(keepSelections).length > 0 && (
              <button className="btn btn--primary" onClick={handleResolve}>
                执行清理 ({Object.keys(keepSelections).length} 组)
              </button>
            )}
          </div>

          {job.groups.map((group: DedupGroup) => (
            <div key={group.group_id} className="dedup-group">
              <div className="dedup-group__header" onClick={() => toggleGroup(group.group_id)}>
                <span>
                  组 {group.group_id.slice(0, 8)} — {group.items.length} 张相似
                  {group.stage && ` (${group.stage})`}
                </span>
                <span>{expandedGroups.has(group.group_id) ? "▼" : "▶"}</span>
              </div>

              {expandedGroups.has(group.group_id) && (
                <div className="dedup-group__items">
                  {group.items.map((item) => (
                    <div
                      key={item.path}
                      className={`dedup-item ${keepSelections[group.group_id] === item.path ? "dedup-item--keep" : ""}`}
                      onClick={() => selectKeep(group.group_id, item.path)}
                    >
                      <div className="dedup-item__thumb">
                        {item.thumbnail_url ? (
                          <img src={apiUrl(item.thumbnail_url)} alt={item.file_name} />
                        ) : (
                          <span>📷</span>
                        )}
                      </div>
                      <div className="dedup-item__info">
                        <div>{item.file_name}</div>
                        <div className="dedup-item__meta">
                          {(item.file_size / 1024 / 1024).toFixed(1)} MB
                          {item.similarity > 0 && ` · 相似度 ${(item.similarity * 100).toFixed(0)}%`}
                        </div>
                      </div>
                      <div className="dedup-item__action">
                        {keepSelections[group.group_id] === item.path ? "✓ 保留" : "点击保留"}
                      </div>
                    </div>
                  ))}
                </div>
              )}
            </div>
          ))}
        </div>
      )}

      {job && job.status === "completed" && job.groups.length === 0 && (
        <div className="card">
          <p>未发现重复照片</p>
        </div>
      )}

      <FolderPicker open={showPicker} onClose={() => setShowPicker(false)} onSelect={handlePickFolder} />
    </div>
  );
}
