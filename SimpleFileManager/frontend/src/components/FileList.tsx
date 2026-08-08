import { type FileNode } from '../api';

interface FileListProps {
  items: FileNode[];
  selectedPath: string | null;
  onSelect: (node: FileNode) => void;
  onDoubleClick: (node: FileNode) => void;
}

function formatSize(bytes: number): string {
  if (bytes === 0) return '0 B';
  const k = 1024;
  const sizes = ['B', 'KB', 'MB', 'GB', 'TB'];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return parseFloat((bytes / Math.pow(k, i)).toFixed(1)) + ' ' + sizes[i];
}

function formatDate(dateStr: string): string {
  try {
    return new Date(dateStr).toLocaleDateString();
  } catch {
    return dateStr;
  }
}

function getFileIcon(node: FileNode): string {
  if (node.is_dir) return '📁';
  const ext = node.extension.toLowerCase();
  const iconMap: Record<string, string> = {
    '.jpg': '🖼️', '.jpeg': '🖼️', '.png': '🖼️', '.gif': '🖼️', '.webp': '🖼️', '.svg': '🖼️',
    '.mp4': '🎬', '.avi': '🎬', '.mkv': '🎬', '.mov': '🎬',
    '.mp3': '🎵', '.wav': '🎵', '.ogg': '🎵', '.flac': '🎵',
    '.pdf': '📄',
    '.doc': '📝', '.docx': '📝',
    '.xls': '📊', '.xlsx': '📊',
    '.zip': '📦', '.tar': '📦', '.gz': '📦', '.7z': '📦', '.rar': '📦',
    '.txt': '📃', '.md': '📃',
    '.json': '📋', '.xml': '📋', '.yaml': '📋', '.yml': '📋',
    '.html': '🌐', '.css': '🎨', '.js': '💻', '.ts': '💻',
    '.py': '🐍', '.rs': '🦀', '.go': '🐹', '.java': '☕',
    '.exe': '⚙️', '.dll': '⚙️',
  };
  return iconMap[ext] || '📄';
}

export function FileList({ items, selectedPath, onSelect, onDoubleClick }: FileListProps) {
  if (items.length === 0) {
    return (
      <div className="empty-state">
        <div className="empty-state-icon">📂</div>
        <p>This folder is empty</p>
      </div>
    );
  }

  return (
    <div className="file-list">
      {items.map((item) => (
        <div
          key={item.path}
          className={`file-item ${selectedPath === item.path ? 'selected' : ''}`}
          onClick={() => onSelect(item)}
          onDoubleClick={() => onDoubleClick(item)}
        >
          <div className="file-icon">{getFileIcon(item)}</div>
          <div className="file-info">
            <div className="file-name">{item.name}</div>
            <div className="file-meta">
              {!item.is_dir && <span className="file-size">{formatSize(item.size)}</span>}
              <span>{formatDate(item.modified)}</span>
            </div>
          </div>
        </div>
      ))}
    </div>
  );
}
