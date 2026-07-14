import { useState, useEffect, useCallback } from "react";
import { useNavigate } from "react-router-dom";
import { listDirs, browseFiles, addDir, removeDir, getOrphanedRaws, deleteOrphanedRaws, getFileSiblings } from "@/api/files";
import { listResults } from "@/api/analysis";
import { apiUrl } from "@/api/client";
import type { DirEntry, BrowseResult, FileNode, AnalysisResult } from "@/api/types";
import { PhotoGrid } from "@/components/PhotoGrid";
import { FolderPicker } from "@/components/FolderPicker";
import { ImagePreview } from "@/components/ImagePreview";
import { appendDirUnique, reportDuplicateDirs } from "@/utils/dirGuard";

type SortKey = "name" | "time" | "score";
type SortDir = "asc" | "desc";

export function Explorer() {
  const navigate = useNavigate();
  const [dirs, setDirs] = useState<DirEntry[]>([]);
  const [activeDir, setActiveDir] = useState<DirEntry | null>(null);
  const [browse, setBrowse] = useState<BrowseResult | null>(null);
  const [selectedPaths, setSelectedPaths] = useState<Set<string>>(new Set());
  const [loading, setLoading] = useState(false);
  const [showPicker, setShowPicker] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [previewItem, setPreviewItem] = useState<FileNode | null>(null);
  const [results, setResults] = useState<Map<string, AnalysisResult>>(new Map());
  const [sortKey, setSortKey] = useState<SortKey>("name");
  const [sortDir, setSortDir] = useState<SortDir>("asc");
  const [orphanedRaws, setOrphanedRaws] = useState<string[]>([]);

  const loadDirs = useCallback(async () => {
    try {
      const result = await listDirs();
      reportDuplicateDirs("Explorer:listDirs", result);
      setDirs(result);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Failed to load dirs");
    }
  }, []);

  useEffect(() => { loadDirs(); }, [loadDirs]);

  useEffect(() => {
    listResults().then((res) => {
      const map = new Map<string, AnalysisResult>();
      for (const r of res) {
        map.set(r.file_path, r);
      }
      setResults(map);
    }).catch(() => {});
  }, []);

  const getSortedItems = useCallback((items: FileNode[]): FileNode[] => {
    const dirs = items.filter((i) => i.is_dir);
    const images = items.filter((i) => !i.is_dir);

    const sorted = [...images].sort((a, b) => {
      let cmp = 0;
      if (sortKey === "name") {
        cmp = a.name.localeCompare(b.name);
      } else if (sortKey === "time") {
        cmp = (a.modified || "").localeCompare(b.modified || "");
      } else if (sortKey === "score") {
        const sa = results.get(a.path)?.data?.score ?? -1;
        const sb = results.get(b.path)?.data?.score ?? -1;
        cmp = sa - sb;
      }
      return sortDir === "asc" ? cmp : -cmp;
    });

    return [...dirs, ...sorted];
  }, [sortKey, sortDir, results]);

  const loadBrowse = useCallback(async (dir: DirEntry, subPath?: string) => {
    setLoading(true);
    try {
      const result = await browseFiles(dir.id, subPath);
      setBrowse(result);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Failed to browse");
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    if (activeDir) loadBrowse(activeDir);
  }, [activeDir, loadBrowse]);

  const handlePickFolder = async (path: string, name: string) => {
    try {
      const dir = await addDir(path, name);
      setDirs((prev) => appendDirUnique(prev, dir, "Explorer:addDir"));
      setActiveDir(dir);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Failed to add dir");
    }
  };

  const handleRemoveDir = async (id: string) => {
    if (!confirm("确定移除此目录？")) return;
    try {
      await removeDir(id);
      setDirs((prev) => prev.filter((d) => d.id !== id));
      if (activeDir?.id === id) setActiveDir(null);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Failed to remove dir");
    }
  };

  const handleNavigate = (item: FileNode) => {
    if (item.is_dir) {
      loadBrowse(activeDir!, item.path);
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

  const goToAnalysis = () => {
    const paths = Array.from(selectedPaths);
    navigate("/analysis", { 
      state: { 
        filePaths: paths, 
        dirPath: browse?.current_path,
        autoStart: true 
      } 
    });
  };

  const handleDeleteSelected = async () => {
    const paths = Array.from(selectedPaths);
    if (paths.length === 0) return;

    try {
      const allPaths: string[] = [];
      for (const path of paths) {
        allPaths.push(path);
        const siblings = await getFileSiblings(path);
        allPaths.push(...siblings.siblings);
      }
      const uniquePaths = [...new Set(allPaths)];
      const fileNames = uniquePaths.map((p: string) => p.split(/[\\/]/).pop()).join(", ");
      if (!confirm(`确定删除 ${uniquePaths.length} 个文件？\n${fileNames}`)) return;

      for (const path of uniquePaths) {
        await fetch(apiUrl(`/files?path=${encodeURIComponent(path)}`), { method: "DELETE" });
      }
      setSelectedPaths(new Set());
      if (activeDir) {
        loadBrowse(activeDir, browse?.current_path);
      }
    } catch {
      alert("删除失败");
    }
  };

  const goUp = () => {
    if (browse?.parent_path && activeDir) {
      loadBrowse(activeDir, browse.parent_path);
    }
  };

  const checkOrphanedRaws = async () => {
    if (!activeDir) return;
    try {
      const data = await getOrphanedRaws(activeDir.id);
      setOrphanedRaws(data.orphaned);
      if (data.count === 0) {
        alert("没有发现孤立的 RAW 文件");
      }
    } catch {
      alert("检查失败");
    }
  };

  const deleteOrphanedRawsHandler = async () => {
    if (!activeDir) return;
    if (orphanedRaws.length === 0) {
      await checkOrphanedRaws();
      return;
    }
    const names = orphanedRaws.map((p) => p.split(/[\\/]/).pop()).join(", ");
    if (!confirm(`确定删除 ${orphanedRaws.length} 个孤立 RAW 文件？\n${names}`)) return;
    try {
      const result = await deleteOrphanedRaws(activeDir.id);
      alert(`已删除 ${result.count} 个文件`);
      setOrphanedRaws([]);
      if (activeDir) {
        loadBrowse(activeDir, browse?.current_path);
      }
    } catch {
      alert("删除失败");
    }
  };

  return (
    <div className="page">
      <h1>浏览照片</h1>
      {error && <div className="error-msg" onClick={() => setError(null)}>{error}</div>}

      <div className="explorer">
        <div className="explorer__sidebar">
          <div className="explorer__dir-header">
            <h3>目录</h3>
            <button className="btn btn--sm" onClick={() => setShowPicker(true)}>+ 添加</button>
          </div>

          <div className="dir-list">
            {dirs.map((dir) => (
              <div
                key={dir.id}
                className={`dir-item ${activeDir?.id === dir.id ? "dir-item--active" : ""}`}
                onClick={() => setActiveDir(dir)}
              >
                <span className="dir-item__icon">📁</span>
                <span className="dir-item__name">{dir.name || dir.path}</span>
                <button
                  className="dir-item__remove"
                  onClick={(e) => { e.stopPropagation(); handleRemoveDir(dir.id); }}
                  title="移除"
                >
                  ✕
                </button>
              </div>
            ))}
            {dirs.length === 0 && <div className="empty-hint">尚未添加目录，点击上方按钮选择文件夹</div>}
          </div>
        </div>

        <div className="explorer__content">
          {activeDir ? (
            <>
              <div className="explorer__toolbar">
                <div className="explorer__breadcrumb">
                  <span className="breadcrumb__root" onClick={() => loadBrowse(activeDir)}>
                    {activeDir.name || activeDir.path}
                  </span>
                  <span className="breadcrumb__sep">›</span>
                  {browse?.current_path && browse.current_path !== activeDir.path && (
                    <span>{browse.current_path.replace(activeDir.path, "")}</span>
                  )}
                </div>
                <div className="explorer__actions">
                  {browse?.parent_path && (
                    <button className="btn btn--sm" onClick={goUp}>↑ 上级</button>
                  )}
                  <span className="sort-label">排序:</span>
                  <select
                    value={sortKey}
                    onChange={(e) => setSortKey(e.target.value as SortKey)}
                    className="sort-select"
                  >
                    <option value="name">文件名</option>
                    <option value="time">时间</option>
                    <option value="score">评分</option>
                  </select>
                  <button
                    className="btn btn--sm sort-dir-btn"
                    onClick={() => setSortDir((d) => (d === "asc" ? "desc" : "asc"))}
                    title={sortDir === "asc" ? "升序" : "降序"}
                  >
                    {sortDir === "asc" ? "↑" : "↓"}
                  </button>
                  <button className="btn btn--sm" onClick={selectAll}>
                    {browse?.items.filter((i) => !i.is_dir).every((i) => selectedPaths.has(i.path))
                      ? "取消全选"
                      : "全选"}
                  </button>
                  {selectedPaths.size > 0 && (
                    <>
                      <button className="btn btn--sm btn--primary" onClick={goToAnalysis}>
                        分析选中 ({selectedPaths.size})
                      </button>
                      <button className="btn btn--sm btn--danger" onClick={handleDeleteSelected}>
                        删除选中 ({selectedPaths.size})
                      </button>
                    </>
                  )}
                  {activeDir && (
                    <button className="btn btn--sm btn--danger" onClick={orphanedRaws.length > 0 ? deleteOrphanedRawsHandler : checkOrphanedRaws}>
                      {orphanedRaws.length > 0 ? `删除孤立RAW (${orphanedRaws.length})` : "删除孤立RAW"}
                    </button>
                  )}
                </div>
              </div>

              {loading ? (
                <div className="loading">加载中...</div>
              ) : browse ? (
                <>
                  {browse.items.filter((i) => i.is_dir).length > 0 && (
                    <div className="folder-list">
                      {browse.items
                        .filter((i) => i.is_dir)
                        .map((item) => (
                          <div
                            key={item.path}
                            className="folder-item"
                            onClick={() => handleNavigate(item)}
                          >
                            <span>📁</span>
                            <span>{item.name}</span>
                          </div>
                        ))}
                    </div>
                  )}
                  <PhotoGrid
                    items={getSortedItems(browse.items)}
                    onSelect={handleNavigate}
                    selectedPaths={selectedPaths}
                    onToggleSelect={toggleSelect}
                    scores={results}
                  />
                </>
              ) : null}
            </>
          ) : (
            <div className="empty-state">请从左侧选择一个目录开始浏览</div>
          )}
        </div>
      </div>

      <FolderPicker
        open={showPicker}
        onClose={() => setShowPicker(false)}
        onSelect={handlePickFolder}
      />

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
