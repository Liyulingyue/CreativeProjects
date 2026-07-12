import { useState } from "react";
import type { BrowseResult, FileNode } from "@/api/types";
import { PhotoGrid } from "@/components/PhotoGrid";

interface FileBrowserProps {
  browse: BrowseResult;
  selectedPaths: Set<string>;
  onToggleSelect: (path: string) => void;
  onSelect: (item: FileNode) => void;
  onSelectAll: () => void;
  onAction: () => void;
  onActionAll: () => void;
  imageCount: number;
  loading: boolean;
  actionLabel: string;
  actionAllLabel: string;
}

export function FileBrowser({
  browse,
  selectedPaths,
  onToggleSelect,
  onSelect,
  onSelectAll,
  onAction,
  onActionAll,
  imageCount,
  loading,
  actionLabel,
  actionAllLabel,
}: FileBrowserProps) {
  const [show, setShow] = useState(true);

  const handleNavigate = (item: FileNode) => {
    onSelect(item);
  };

  return (
    <div className="card card--collapsible">
      <div className="card__header" onClick={() => setShow((v) => !v)}>
        <div className="file-browser__info">
          <span>{show ? "▼" : "▶"}</span>
          <span className="file-browser__count">{imageCount} 张图片</span>
          {selectedPaths.size > 0 && (
            <span className="file-browser__selected">已选 {selectedPaths.size} 张</span>
          )}
        </div>
        {show && (
          <div className="file-browser__actions" onClick={(e) => e.stopPropagation()}>
            <button className="btn btn--sm" onClick={onSelectAll}>
              {imageCount > 0 && browse.items.filter((i) => !i.is_dir).every((i) => selectedPaths.has(i.path))
                ? "取消全选"
                : "全选"}
            </button>
            <button
              className="btn btn--sm btn--primary"
              onClick={onAction}
              disabled={loading || selectedPaths.size === 0}
            >
              {actionLabel.replace("{n}", String(selectedPaths.size))}
            </button>
            <button
              className="btn btn--sm"
              onClick={onActionAll}
              disabled={loading || imageCount === 0}
            >
              {actionAllLabel}
            </button>
          </div>
        )}
      </div>

      {show && (
        <>
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
            onToggleSelect={onToggleSelect}
          />
        </>
      )}
    </div>
  );
}
