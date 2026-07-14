import type { FileNode, AnalysisResult } from "@/api/types";
import { resolveApiUrl } from "@/api/client";

interface PhotoGridProps {
  items: FileNode[];
  onSelect: (item: FileNode) => void;
  selectedPaths: Set<string>;
  onToggleSelect: (path: string) => void;
  scores?: Map<string, AnalysisResult>;
}

export function PhotoGrid({ items, onSelect, selectedPaths, onToggleSelect, scores }: PhotoGridProps) {
  const imageItems = items.filter((i) => !i.is_dir);

  if (imageItems.length === 0) {
    return <div className="empty-state">当前目录没有图片</div>;
  }

  return (
    <div className="photo-grid">
      {imageItems.map((item) => {
        const isSelected = selectedPaths.has(item.path);
        const score = scores?.get(item.path)?.data?.score;
        return (
          <div
            key={item.path}
            className={`photo-card ${isSelected ? "photo-card--selected" : ""}`}
            onClick={() => onSelect(item)}
          >
            <div className="photo-card__check" onClick={(e) => { e.stopPropagation(); onToggleSelect(item.path); }}>
              {isSelected && "✓"}
            </div>
            <div className="photo-card__image">
              {item.thumbnail_url ? (
                <img src={resolveApiUrl(item.thumbnail_url)} alt={item.name} loading="lazy" />
              ) : (
                <div className="photo-card__placeholder">📷</div>
              )}
              {score !== undefined && score >= 0 && (
                <div
                  className={`photo-card__score ${score >= 70 ? "photo-card__score--good" : score >= 40 ? "photo-card__score--mid" : "photo-card__score--low"}`}
                >
                  {score}
                </div>
              )}
            </div>
            <div className="photo-card__info">
              <span className="photo-card__name" title={item.name}>{item.name}</span>
              <span className="photo-card__size">{formatSize(item.size)}</span>
            </div>
          </div>
        );
      })}
    </div>
  );
}

function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}
