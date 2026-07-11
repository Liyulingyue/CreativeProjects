import { useState, useEffect, useCallback } from "react";
import { useNavigate } from "react-router-dom";
import { listDirs, browseFiles, addDir, removeDir } from "@/api/files";
import type { DirEntry, BrowseResult, FileNode } from "@/api/types";
import { PhotoGrid } from "@/components/PhotoGrid";

export function Explorer() {
  const navigate = useNavigate();
  const [dirs, setDirs] = useState<DirEntry[]>([]);
  const [activeDir, setActiveDir] = useState<DirEntry | null>(null);
  const [browse, setBrowse] = useState<BrowseResult | null>(null);
  const [selectedPaths, setSelectedPaths] = useState<Set<string>>(new Set());
  const [loading, setLoading] = useState(false);
  const [showAddDir, setShowAddDir] = useState(false);
  const [newDirPath, setNewDirPath] = useState("");
  const [newDirName, setNewDirName] = useState("");
  const [error, setError] = useState<string | null>(null);

  const loadDirs = useCallback(async () => {
    try {
      const result = await listDirs();
      setDirs(result);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Failed to load dirs");
    }
  }, []);

  useEffect(() => { loadDirs(); }, [loadDirs]);

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

  const handleAddDir = async () => {
    if (!newDirPath.trim()) return;
    try {
      const dir = await addDir(newDirPath.trim(), newDirName.trim() || undefined);
      setDirs((prev) => [...prev, dir]);
      setActiveDir(dir);
      setShowAddDir(false);
      setNewDirPath("");
      setNewDirName("");
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
    navigate("/analysis", { state: { filePaths: paths } });
  };

  const goUp = () => {
    if (browse?.parent_path && activeDir) {
      loadBrowse(activeDir, browse.parent_path);
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
            <button className="btn btn--sm" onClick={() => setShowAddDir(true)}>+ 添加</button>
          </div>

          {showAddDir && (
            <div className="add-dir-form">
              <input
                placeholder="路径 (如 /mnt/nas/photos)"
                value={newDirPath}
                onChange={(e) => setNewDirPath(e.target.value)}
              />
              <input
                placeholder="名称 (可选)"
                value={newDirName}
                onChange={(e) => setNewDirName(e.target.value)}
              />
              <div className="add-dir-form__actions">
                <button className="btn btn--sm btn--primary" onClick={handleAddDir}>确定</button>
                <button className="btn btn--sm" onClick={() => setShowAddDir(false)}>取消</button>
              </div>
            </div>
          )}

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
            {dirs.length === 0 && <div className="empty-hint">尚未添加目录，点击上方按钮添加</div>}
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
                  {browse?.current_path && browse.current_path !== activeDir.path && (
                    <span className="breadcrumb__sep">/</span>
                  )}
                  {browse?.current_path && browse.current_path !== activeDir.path && (
                    <span>{browse.current_path.replace(activeDir.path, "")}</span>
                  )}
                </div>
                <div className="explorer__actions">
                  {browse?.parent_path && (
                    <button className="btn btn--sm" onClick={goUp}>↑ 上级</button>
                  )}
                  <button className="btn btn--sm" onClick={selectAll}>
                    {browse?.items.filter((i) => !i.is_dir).every((i) => selectedPaths.has(i.path))
                      ? "取消全选"
                      : "全选"}
                  </button>
                  {selectedPaths.size > 0 && (
                    <button className="btn btn--sm btn--primary" onClick={goToAnalysis}>
                      分析选中 ({selectedPaths.size})
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
                    items={browse.items}
                    onSelect={handleNavigate}
                    selectedPaths={selectedPaths}
                    onToggleSelect={toggleSelect}
                  />
                </>
              ) : null}
            </>
          ) : (
            <div className="empty-state">请从左侧选择一个目录开始浏览</div>
          )}
        </div>
      </div>
    </div>
  );
}
