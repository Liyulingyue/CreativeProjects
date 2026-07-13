import { useState, useEffect, useCallback } from "react";
import { startDedupFolder, startDedupPaths, getDedupJob, getDedupJobByDir, resolveDedupGroups } from "@/api/dedup";
import { listDirs, addDir, browseFiles } from "@/api/files";
import { listResults } from "@/api/analysis";
import type { DirEntry, DedupJob, DedupGroup, BrowseResult, FileNode, AnalysisResult } from "@/api/types";
import { PathInput } from "@/components/PathInput";
import { FolderPicker } from "@/components/FolderPicker";
import { ImagePreview } from "@/components/ImagePreview";
import { FileBrowser } from "@/components/FileBrowser";
import { appendDirUnique, reportDuplicateDirs } from "@/utils/dirGuard";

function getFileName(path: string): string {
  return path.split(/[\\/]/).pop() ?? path;
}

function getFileStem(path: string): string {
  const name = getFileName(path);
  const dotIndex = name.lastIndexOf(".");
  return dotIndex > 0 ? name.substring(0, dotIndex) : name;
}

export function Dedup() {
  const [dirs, setDirs] = useState<DirEntry[]>([]);
  const [pathValue, setPathValue] = useState("");
  const [currentDir, setCurrentDir] = useState<DirEntry | null>(null);
  const [browse, setBrowse] = useState<BrowseResult | null>(null);
  const [selectedPaths, setSelectedPaths] = useState<Set<string>>(new Set());
  const [job, setJob] = useState<DedupJob | null>(null);
  const [expandedGroups, setExpandedGroups] = useState<Set<string>>(new Set());
  const [selectedForDeletion, setSelectedForDeletion] = useState<Set<string>>(new Set());
  const [loading, setLoading] = useState(false);
  const [showPicker, setShowPicker] = useState(false);
  const [previewItem, setPreviewItem] = useState<FileNode | null>(null);
  const [results, setResults] = useState<Map<string, AnalysisResult>>(new Map());

  useEffect(() => {
    listDirs()
      .then((result) => {
        reportDuplicateDirs("Dedup:listDirs", result);
        setDirs(result);
      })
      .catch(() => {});
    listResults().then((res) => {
      const map = new Map<string, AnalysisResult>();
      for (const r of res) {
        map.set(r.file_path, r);
      }
      setResults(map);
    }).catch(() => {});
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
          setDirs((prev) => {
            if (!dir) return prev;
            return appendDirUnique(prev, dir!, "Dedup:addDir");
          });
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
    loadExistingDedup(dir.id);
  };

  const loadExistingDedup = async (dirId: string) => {
    try {
      const existingJob = await getDedupJobByDir(dirId);
      setJob(existingJob);
      setExpandedGroups(new Set(existingJob.groups.map((g) => g.group_id)));
    } catch {
      setJob(null);
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

  const toggleAllGroups = (expand: boolean) => {
    if (!job) return;
    if (expand) {
      setExpandedGroups(new Set(job.groups.map((g) => g.group_id)));
    } else {
      setExpandedGroups(new Set());
    }
  };

  const toggleItemSelection = (path: string) => {
    setSelectedForDeletion((prev) => {
      const next = new Set(prev);
      if (next.has(path)) next.delete(path);
      else next.add(path);
      return next;
    });
  };

  const selectAllInGroup = (group: DedupGroup, select: boolean) => {
    setSelectedForDeletion((prev) => {
      const next = new Set(prev);
      for (const item of group.items) {
        if (select) next.add(item.path);
        else next.delete(item.path);
      }
      return next;
    });
  };

  const handleResolve = async () => {
    if (!job) return;
    const toDelete = Array.from(selectedForDeletion);
    if (toDelete.length === 0) return;

    try {
      for (const path of toDelete) {
        await fetch(`/api/files?path=${encodeURIComponent(path)}`, { method: "DELETE" });
      }

      const actions = job.groups.map((group) => {
        const keep = group.items.find((i) => !selectedForDeletion.has(i.path));
        const remove = group.items.filter((i) => selectedForDeletion.has(i.path)).map((i) => i.path);
        return {
          group_id: group.group_id,
          keep: keep?.path ?? group.items[0].path,
          remove,
        };
      }).filter((a) => a.remove.length > 0);

      await resolveDedupGroups(job.job_id, actions);

      setJob((prev) => {
        if (!prev) return prev;
        const toDeleteSet = new Set(toDelete);
        const updatedGroups = prev.groups.map((g) => ({
          ...g,
          items: g.items.filter((i) => !toDeleteSet.has(i.path)),
        })).filter((g) => g.items.length > 1);
        return {
          ...prev,
          groups: updatedGroups,
          groups_count: updatedGroups.length,
        };
      });
      setSelectedForDeletion(new Set());
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
          actionLabel="去重选中 ({n})"
          actionAllLabel="去重全部"
        />
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
            <div className="section-header__actions">
              <button className="btn btn--sm" onClick={() => toggleAllGroups(true)}>全部展开</button>
              <button className="btn btn--sm" onClick={() => toggleAllGroups(false)}>全部折叠</button>
              <span className="section-header__separator" />
              <button className="btn btn--sm" onClick={() => {
                const allSelected = job.groups.flatMap((g) => g.items).every((i) => selectedForDeletion.has(i.path));
                job.groups.forEach((g) => selectAllInGroup(g, !allSelected));
              }}>反选</button>
              <span className={`dedup-selected-count ${selectedForDeletion.size > 0 ? "" : "btn--hidden"}`}>已选 {selectedForDeletion.size} 项</span>
              <button
                className={`btn btn--danger ${selectedForDeletion.size > 0 ? "" : "btn--hidden"}`}
                onClick={handleResolve}
              >
                删除选中 ({selectedForDeletion.size})
              </button>
            </div>
          </div>

          {job.groups.map((group: DedupGroup) => {
            const totalSize = group.items.reduce((sum, i) => sum + i.file_size, 0);
            const avgSize = totalSize / group.items.length;
            const sizeSaving = totalSize - avgSize;
            const selectedCount = group.items.filter((i) => selectedForDeletion.has(i.path)).length;
            return (
              <div key={group.group_id} className="dedup-group">
                <div className="dedup-group__header" onClick={() => toggleGroup(group.group_id)}>
                  <div className="dedup-group__header-info">
                    <span className="dedup-group__header-title">
                      {expandedGroups.has(group.group_id) ? "▼" : "▶"} 组 {group.group_id.slice(0, 8)}
                    </span>
                    <span className="dedup-group__header-meta">
                      {group.items.length} 张 · 可节省 {(sizeSaving / 1024 / 1024).toFixed(0)} MB
                    </span>
                    {group.stage && <span className="dedup-group__header-stage">{group.stage}</span>}
                  </div>
                  <div className="dedup-group__header-actions" onClick={(e) => e.stopPropagation()}>
                    <label className="dedup-group__select-all">
                      <input
                        type="checkbox"
                        checked={selectedCount === group.items.length && group.items.length > 0}
                        ref={(el) => { if (el) el.indeterminate = selectedCount > 0 && selectedCount < group.items.length; }}
                        onChange={(e) => selectAllInGroup(group, e.target.checked)}
                      />
                      全选本组
                    </label>
                    <button
                      className={`btn btn--sm btn--danger ${selectedCount > 0 ? "" : "btn--hidden"}`}
                      onClick={() => {
                        const toDelete = group.items.filter((i) => selectedForDeletion.has(i.path)).map((i) => i.path);
                        if (toDelete.length === 0) return;
                        const keep = group.items.find((i) => !selectedForDeletion.has(i.path));
                        resolveDedupGroups(job.job_id, [{
                          group_id: group.group_id,
                          keep: keep?.path ?? group.items[0].path,
                          remove: toDelete,
                        }]).then(() => {
                          setJob((prev) => {
                            if (!prev) return prev;
                            return {
                              ...prev,
                              groups: prev.groups.map((g) =>
                                g.group_id === group.group_id
                                  ? { ...g, items: g.items.filter((i) => !selectedForDeletion.has(i.path)) }
                                  : g
                              ).filter((g) => g.items.length > 1),
                            };
                          });
                          setSelectedForDeletion((prev) => {
                            const next = new Set(prev);
                            toDelete.forEach((p) => next.delete(p));
                            return next;
                          });
                        }).catch((e) => alert(e instanceof Error ? e.message : "删除失败"));
                      }}
                    >删除本组选中</button>
                  </div>
                </div>
                {expandedGroups.has(group.group_id) && (
                  <div className="dedup-group__items">
                    {group.items.map((item) => (
                      <div
                        key={item.path}
                        className={`dedup-item ${selectedForDeletion.has(item.path) ? "dedup-item--selected" : ""}`}
                        onClick={() => toggleItemSelection(item.path)}
                      >
                        <div className="dedup-item__checkbox">
                          <input
                            type="checkbox"
                            checked={selectedForDeletion.has(item.path)}
                            onChange={() => toggleItemSelection(item.path)}
                            onClick={(e) => e.stopPropagation()}
                          />
                        </div>
                        <div
                          className="dedup-item__thumb"
                          onClick={(e) => { e.stopPropagation(); setPreviewItem({ name: item.file_name, path: item.path, size: item.file_size, is_dir: false, modified: "", thumbnail_url: item.thumbnail_url }); }}
                        >
                          {item.thumbnail_url ? (
                            <img src={item.thumbnail_url} alt={item.file_name} />
                          ) : (
                            <span>📷</span>
                          )}
                          <div className="dedup-item__preview-hint">🔍</div>
                          {(() => {
                            const r = results.get(item.path);
                            if (r?.success && r.data) {
                              const score = r.data.score;
                              const cls = score >= 70 ? "score--good" : score >= 40 ? "score--mid" : "score--low";
                              return <div className={`dedup-item__score ${cls}`}>{score}</div>;
                            }
                            return null;
                          })()}
                        </div>
                        <div className="dedup-item__info">
                          <div>{item.file_name}</div>
                          <div className="dedup-item__meta">
                            {(() => {
                              const r = results.get(item.path);
                              if (r?.success && r.data) {
                                return <span>{r.data.score}分 · {r.data.blurry} · {r.data.style}</span>;
                              }
                              return <span className="text-muted">未评分</span>;
                            })()}
                          </div>
                          <div className="dedup-item__meta">
                            {(item.file_size / 1024 / 1024).toFixed(1)} MB
                            {item.similarity > 0 && ` · 相似度 ${(item.similarity * 100).toFixed(0)}%`}
                            {item.siblings.length > 0 && ` · 同照片 ${item.siblings.length + 1} 个格式`}
                          </div>
                        </div>
                          <div className="dedup-item__actions">
                            <button
                              className="btn btn--sm btn--danger"
                              onClick={async (e) => {
                                e.stopPropagation();
                                try {
                                  const res = await fetch(`/api/files?path=${encodeURIComponent(item.path)}`, { method: "DELETE" });
                                  if (res.status === 404) {
                                    alert(`文件不存在或已被删除`);
                                    const allPaths = [item.path];
                                    setJob((prev) => {
                                      if (!prev) return prev;
                                      return {
                                        ...prev,
                                        groups: prev.groups.map((g) =>
                                          g.group_id === group.group_id
                                            ? { ...g, items: g.items.filter((i) => !allPaths.includes(i.path)) }
                                            : g
                                        ).filter((g) => g.items.length > 1),
                                      };
                                    });
                                    return;
                                  }
                                  if (!res.ok) {
                                    alert("删除失败");
                                    return;
                                  }
                                  const data = await res.json();
                                  const allPaths: string[] = data.deleted || [item.path];

                                  if (job) {
                                    const keep = group.items.find((i) => !allPaths.includes(i.path) && !allPaths.some((ap) => getFileStem(i.path) === getFileStem(ap)));
                                    await resolveDedupGroups(job.job_id, [{
                                      group_id: group.group_id,
                                      keep: keep?.path ?? group.items[0].path,
                                      remove: allPaths,
                                    }]);
                                  }

                                  setJob((prev) => {
                                    if (!prev) return prev;
                                    return {
                                      ...prev,
                                      groups: prev.groups.map((g) =>
                                        g.group_id === group.group_id
                                          ? { ...g, items: g.items.filter((i) => !allPaths.includes(i.path) && !allPaths.some((ap) => getFileStem(i.path) === getFileStem(ap))) }
                                          : g
                                      ).filter((g) => g.items.length > 1),
                                    };
                                  });
                                  setSelectedForDeletion((prev) => {
                                    const next = new Set(prev);
                                    allPaths.forEach((p: string) => next.delete(p));
                                    return next;
                                  });
                                } catch {
                                  alert("删除失败");
                                }
                              }}
                              title="删除"
                            >
                              🗑
                            </button>
                          <button
                            className="btn btn--sm"
                            onClick={(e) => { e.stopPropagation(); setPreviewItem({ name: item.file_name, path: item.path, size: item.file_size, is_dir: false, modified: "", thumbnail_url: item.thumbnail_url }); }}
                            title="详情"
                          >
                            ℹ
                          </button>
                        </div>
                      </div>
                    ))}
                  </div>
                )}
              </div>
            );
          })}
        </div>
      )}

      {job && job.status === "completed" && job.groups.length === 0 && (
        <div className="card">
          <p>未发现重复照片</p>
        </div>
      )}

      <FolderPicker open={showPicker} onClose={() => setShowPicker(false)} onSelect={handlePickFolder} />

      <ImagePreview
        item={previewItem}
        onClose={() => setPreviewItem(null)}
        onAnalysisComplete={() => {
          listResults().then((res) => {
            const map = new Map<string, AnalysisResult>();
            for (const r of res) {
              map.set(r.file_path, r);
            }
            setResults(map);
          }).catch(() => {});
        }}
      />
    </div>
  );
}
