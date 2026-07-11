import { useState, useEffect, useCallback } from "react";
import { browseFs } from "@/api/fs";
import type { FsBrowseResult, FsEntry } from "@/api/types";

interface FolderPickerProps {
  open: boolean;
  onClose: () => void;
  onSelect: (path: string, name: string) => void;
}

export function FolderPicker({ open, onClose, onSelect }: FolderPickerProps) {
  const [data, setData] = useState<FsBrowseResult | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [selected, setSelected] = useState<string | null>(null);

  const load = useCallback(async (path?: string) => {
    setLoading(true);
    setError(null);
    setSelected(null);
    try {
      const result = await browseFs(path, false);
      setData(result);
    } catch (e) {
      setError(e instanceof Error ? e.message : "加载失败");
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    if (open) load();
  }, [open, load]);

  if (!open) return null;

  const handleNavigate = (entry: FsEntry) => {
    if (entry.is_dir) {
      load(entry.path);
    }
  };

  const handleGoUp = () => {
    if (data?.parent_path !== undefined && data?.parent_path !== null) {
      load(data.parent_path);
    } else if (data?.current_path) {
      const parent = data.current_path.split("/").slice(0, -1).join("/");
      if (parent) load(parent);
    }
  };

  const handleConfirm = () => {
    if (selected) {
      const entry = data?.entries.find((e) => e.path === selected);
      onSelect(selected, entry?.name ?? selected.split("/").pop() ?? selected);
      onClose();
    }
  };

  const handleUseCurrent = () => {
    if (data?.current_path) {
      const name = data.current_path.split("/").pop() ?? data.current_path;
      onSelect(data.current_path, name);
      onClose();
    }
  };

  return (
    <div className="overlay overlay--visible" onClick={onClose}>
      <div className="folder-picker" onClick={(e) => e.stopPropagation()}>
        <div className="folder-picker__header">
          <h3>选择文件夹</h3>
          <button onClick={onClose}>✕</button>
        </div>

        <div className="folder-picker__breadcrumb">
          <span className="breadcrumb__path" title={data?.current_path}>
            {data?.current_path || "选择起始位置"}
          </span>
        </div>

        <div className="folder-picker__actions">
          {data?.current_path && (
            <button className="btn btn--sm" onClick={handleGoUp}>
              ↑ 上级
            </button>
          )}
          {data?.current_path && (
            <button className="btn btn--sm btn--primary" onClick={handleUseCurrent}>
              使用此文件夹
            </button>
          )}
        </div>

        <div className="folder-picker__list">
          {loading && <div className="loading">加载中...</div>}
          {error && <div className="error-msg">{error}</div>}

          {data?.entries.map((entry) => (
            <div
              key={entry.path}
              className={`folder-picker__item ${selected === entry.path ? "folder-picker__item--selected" : ""} ${entry.is_dir ? "folder-picker__item--dir" : "folder-picker__item--file"}`}
              onClick={() => {
                if (entry.is_dir) {
                  setSelected(entry.path);
                  handleNavigate(entry);
                }
              }}
            >
              <span className="folder-picker__icon">
                {entry.is_dir ? "📁" : "🖼"}
              </span>
              <span className="folder-picker__name">{entry.name}</span>
              {entry.children_count !== null && entry.children_count !== undefined && (
                <span className="folder-picker__count">{entry.children_count}</span>
              )}
              {entry.is_dir && <span className="folder-picker__arrow">→</span>}
            </div>
          ))}

          {!loading && data?.entries.length === 0 && (
            <div className="empty-hint">空目录</div>
          )}
        </div>

        {selected && (
          <div className="folder-picker__footer">
            <span className="folder-picker__selected" title={selected}>
              已选: {selected}
            </span>
            <button className="btn btn--primary" onClick={handleConfirm}>
              确认选择
            </button>
          </div>
        )}
      </div>
    </div>
  );
}
