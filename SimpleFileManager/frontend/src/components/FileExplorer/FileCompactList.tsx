import type { MouseEvent, DragEvent } from 'react';
import type { FileNode } from '../../api';

interface FileCompactListProps {
  items: FileNode[];
  selectedPath: string | null;
  onSelect: (node: FileNode) => void;
  onDoubleClick: (node: FileNode) => void;
  onContextMenu: (e: MouseEvent, node: FileNode) => void;
  onDragStart: (e: DragEvent, node: FileNode) => void;
  onDrop: (e: DragEvent, targetFolder: string) => void;
  onRename: (node: FileNode) => void;
  onDelete: (node: FileNode) => void;
  dragOverFolder: string | null;
  setDragOverFolder: (folder: string | null) => void;
  onBack: () => void;
  hasParent: boolean;
}

function formatSize(bytes: number): string {
  if (bytes === 0) return '';
  const k = 1024;
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return parseFloat((bytes / Math.pow(k, i)).toFixed(0)) + ' KB';
}

function formatDate(dateStr: string): string {
  try {
    return new Date(dateStr).toLocaleDateString();
  } catch {
    return dateStr;
  }
}

function getFileIcon(node: FileNode): string {
  if (node.is_dir) return '📂';
  const ext = node.extension.toLowerCase();
  const iconMap: Record<string, string> = {
    '.jpg': '🖼️', '.jpeg': '🖼️', '.png': '🖼️', '.gif': '🖼️', '.webp': '🖼️', '.svg': '🖼️',
    '.mp4': '🎬', '.avi': '🎬', '.mkv': '🎬', '.mov': '🎬',
    '.mp3': '🎵', '.wav': '🎵', '.ogg': '🎵', '.flac': '🎵',
    '.pdf': '📄', '.doc': '📝', '.docx': '📝',
    '.xls': '📊', '.xlsx': '📊',
    '.zip': '📦', '.tar': '📦', '.gz': '📦', '.7z': '📦', '.rar': '📦',
    '.txt': '📃', '.md': '📃',
    '.json': '📋', '.xml': '📋', '.yaml': '📋', '.yml': '📋',
    '.html': '🌐', '.css': '🎨', '.js': '💻', '.ts': '💻',
    '.py': '🐍', '.rs': '🦀', '.go': '🐹', '.java': '☕',
  };
  return iconMap[ext] || '📄';
}

export default function FileCompactList({
  items, selectedPath, onSelect, onDoubleClick, onContextMenu,
  onDragStart, onDrop, onRename, onDelete, dragOverFolder, setDragOverFolder,
  onBack, hasParent
}: FileCompactListProps) {
  const folders = items.filter(i => i.is_dir);
  const files = items.filter(i => !i.is_dir);

  return (
    <div className="flex flex-col space-y-1 p-2">
      {/* Compact View Header */}
      <div className="flex items-center px-4 py-1.5 bg-slate-50/50 rounded-lg text-[8px] font-bold text-slate-400 uppercase tracking-tight border border-slate-100/50 mb-1">
        <div className="w-8"></div>
        <div className="flex-1">名称</div>
        <div className="w-16 text-center">类型</div>
        <div className="w-16 text-center">大小</div>
        <div className="w-24 text-center">修改日期</div>
        <div className="w-20 text-right pr-2">操作</div>
      </div>

      {/* Back Button */}
      {hasParent && (
        <div
          onClick={onBack}
          onDragOver={(e) => e.preventDefault()}
          onDrop={(e) => {
            e.preventDefault();
            e.stopPropagation();
          }}
          className="group flex items-center px-4 py-2 transition-all border bg-slate-50/50 border-slate-100 hover:shadow-sm rounded-lg cursor-pointer hover:bg-slate-100"
        >
          <div className="w-8 flex items-center justify-center">
            <span className="text-base group-hover:scale-110 transition-transform">⬆</span>
          </div>
          <div className="flex-1 text-[10px] font-bold text-slate-600 uppercase">..</div>
          <div className="w-16 text-[8px] text-slate-400 font-medium uppercase text-center">BACK</div>
          <div className="w-16 text-[8px] text-slate-300 font-medium text-center">--</div>
          <div className="w-24 text-[8px] text-slate-300 font-medium text-center">--</div>
          <div className="w-20 flex justify-end">
            <span className="text-[7px] text-slate-300 mr-1">返回</span>
          </div>
        </div>
      )}

      {/* Folders */}
      {folders.map(folder => {
        const isDragOver = dragOverFolder === folder.path;
        const isSelected = selectedPath === folder.path;

        return (
          <div
            key={folder.path}
            draggable
            onDragStart={(e) => onDragStart(e, folder)}
            onDragOver={(e) => {
              e.preventDefault();
              setDragOverFolder(folder.path);
            }}
            onDragLeave={() => setDragOverFolder(null)}
            onDrop={(e) => {
              setDragOverFolder(null);
              onDrop(e, folder.path);
            }}
            onClick={() => onSelect(folder)}
            onDoubleClick={() => onDoubleClick(folder)}
            onContextMenu={(e) => onContextMenu(e, folder)}
            className={`group flex items-center px-4 py-2 transition-all border rounded-lg cursor-pointer ${
              isDragOver
                ? 'bg-indigo-50 border-indigo-400 translate-x-1'
                : isSelected
                  ? 'bg-indigo-50 border-indigo-200'
                  : 'bg-white border-slate-50 hover:shadow-sm'
            }`}
          >
            <div className="w-8 flex items-center justify-center">
              <span className="text-base group-hover:scale-110 transition-transform">{getFileIcon(folder)}</span>
            </div>
            <div className="flex-1 text-[10px] font-bold text-slate-700 tracking-tight truncate">{folder.name}</div>
            <div className="w-16 text-[8px] text-slate-400 font-medium uppercase text-center">FOLDER</div>
            <div className="w-16 text-[8px] text-slate-300 font-medium text-center">--</div>
            <div className="w-24 text-[8px] text-slate-300 font-medium text-center">{formatDate(folder.modified)}</div>
            <div className="w-20 flex justify-end space-x-1 opacity-0 group-hover:opacity-100 transition-opacity">
              <button
                onClick={(e) => { e.stopPropagation(); onRename(folder); }}
                className="w-5 h-5 flex items-center justify-center rounded bg-slate-100 text-slate-600 hover:bg-slate-200 active:scale-90 transition-all text-[7px]"
              >✏️</button>
              <button
                onClick={(e) => { e.stopPropagation(); onDelete(folder); }}
                className="w-5 h-5 flex items-center justify-center rounded bg-red-100 text-red-600 hover:bg-red-200 active:scale-90 transition-all text-[7px]"
              >✕</button>
            </div>
          </div>
        );
      })}

      {/* Files */}
      {files.map(file => {
        const isSelected = selectedPath === file.path;

        return (
          <div
            key={file.path}
            draggable
            onDragStart={(e) => onDragStart(e, file)}
            onClick={() => onSelect(file)}
            onDoubleClick={() => onDoubleClick(file)}
            onContextMenu={(e) => onContextMenu(e, file)}
            className={`group flex items-center px-4 py-2 rounded-lg transition-all border ${
              isSelected
                ? 'bg-indigo-50 border-indigo-200'
                : 'bg-white border-slate-50 hover:shadow-sm'
            }`}
          >
            <div className="w-8 flex items-center justify-center">
              <span className="text-base group-hover:scale-110 transition-transform">{getFileIcon(file)}</span>
            </div>
            <div className="flex-1 text-[10px] font-bold text-slate-800 tracking-tight truncate">{file.name}</div>
            <div className="w-16 text-[8px] text-slate-400 font-medium uppercase text-center">{file.extension.slice(1) || 'FILE'}</div>
            <div className="w-16 text-[8px] text-slate-500 font-medium text-center">{formatSize(file.size)}</div>
            <div className="w-24 text-[8px] text-slate-400 font-medium text-center">{formatDate(file.modified)}</div>
            <div className="w-20 flex justify-end space-x-1 opacity-0 group-hover:opacity-100 transition-opacity">
              <button className="w-5 h-5 flex items-center justify-center rounded bg-indigo-600 text-white shadow-sm hover:bg-indigo-700 active:scale-90 transition-all text-[7px]">⬇</button>
              <button
                onClick={(e) => { e.stopPropagation(); onRename(file); }}
                className="w-5 h-5 flex items-center justify-center rounded bg-slate-100 text-slate-600 hover:bg-slate-200 active:scale-90 transition-all text-[7px]"
              >✏️</button>
              <button
                onClick={(e) => { e.stopPropagation(); onDelete(file); }}
                className="w-5 h-5 flex items-center justify-center rounded bg-red-100 text-red-600 hover:bg-red-200 active:scale-90 transition-all text-[7px]"
              >✕</button>
            </div>
          </div>
        );
      })}
    </div>
  );
}
