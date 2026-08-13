import type { MouseEvent, DragEvent } from 'react';
import type { FileNode } from '../../api';

interface FileGridProps {
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
  isCreatingFolder: boolean;
  newFolderName: string;
  setNewFolderName: (name: string) => void;
  onCreateFolder: () => void;
  onCancelCreateFolder: () => void;
  onBack: () => void;
  hasParent: boolean;
}

function formatSize(bytes: number): string {
  if (bytes === 0) return '';
  const k = 1024;
  const sizes = ['B', 'KB', 'MB', 'GB', 'TB'];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return parseFloat((bytes / Math.pow(k, i)).toFixed(1)) + ' ' + sizes[i];
}

function getFileIcon(node: FileNode): string {
  if (node.is_dir) return '📁';
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

export default function FileGrid({
  items, selectedPath, onSelect, onDoubleClick, onContextMenu,
  onDragStart, onDrop, onRename, onDelete, dragOverFolder, setDragOverFolder,
  isCreatingFolder, newFolderName, setNewFolderName, onCreateFolder, onCancelCreateFolder,
  onBack, hasParent
}: FileGridProps) {
  const folders = items.filter(i => i.is_dir);
  const files = items.filter(i => !i.is_dir);

  return (
    <div className="grid grid-cols-2 sm:grid-cols-4 md:grid-cols-6 lg:grid-cols-8 xl:grid-cols-10 gap-2 p-4">
      {/* Back Button */}
      {hasParent && (
        <div
          onClick={onBack}
          onDragOver={(e) => e.preventDefault()}
          onDrop={(e) => {
            e.preventDefault();
            e.stopPropagation();
          }}
          className="group flex flex-col items-center p-4 rounded-2xl hover:bg-white hover:shadow-xl cursor-pointer transition-all border border-transparent hover:border-slate-100"
        >
          <div className="text-5xl mb-2 opacity-50 group-hover:opacity-100">📁</div>
          <span className="text-[10px] font-black text-slate-500 uppercase">..</span>
          <span className="text-[8px] text-slate-300 font-bold uppercase">上级目录</span>
        </div>
      )}

      {/* New Folder Placeholder */}
      {isCreatingFolder && (
        <div className="flex flex-col items-center p-4 rounded-2xl bg-white shadow-xl ring-2 ring-indigo-500 relative group/new">
          <div className="text-5xl mb-2 animate-pulse">📂</div>
          <input
            autoFocus
            value={newFolderName}
            onChange={(e) => setNewFolderName(e.target.value)}
            onBlur={onCreateFolder}
            onKeyDown={(e) => {
              if (e.key === 'Enter') onCreateFolder();
              if (e.key === 'Escape') onCancelCreateFolder();
            }}
            placeholder="名称..."
            className="w-full bg-transparent text-center text-[10px] font-black outline-none text-indigo-700 uppercase"
          />
          <button
            onClick={onCancelCreateFolder}
            className="absolute -top-2 -right-2 w-6 h-6 bg-white border border-slate-100 rounded-full flex items-center justify-center text-[8px] shadow-sm hover:bg-slate-50"
          >
            ✕
          </button>
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
            className={`group relative flex flex-col items-center p-4 rounded-2xl transition-all border-2 cursor-pointer ${
              isDragOver
                ? 'bg-indigo-50 border-indigo-400 scale-110 z-10 shadow-2xl'
                : isSelected
                  ? 'bg-indigo-50 border-indigo-200 shadow-lg'
                  : 'hover:bg-white hover:shadow-xl border-transparent hover:scale-105'
            }`}
          >
            <div className="text-5xl mb-2 transition-transform duration-500 group-hover:rotate-12">{getFileIcon(folder)}</div>
            <span className="text-[11px] font-black text-slate-700 truncate w-full text-center px-2 tracking-tight">
              {folder.name}
            </span>

            {/* Hover Actions */}
            <div className="absolute -bottom-2 opacity-0 group-hover:opacity-100 transition-all flex space-x-2 bg-white px-3 py-1.5 rounded-2xl shadow-xl border border-slate-50 z-10">
              <button
                onClick={(e) => {
                  e.stopPropagation();
                  onRename(folder);
                }}
                className="text-[10px] grayscale hover:grayscale-0 transition-all"
              >
                ✏️
              </button>
              <button
                onClick={(e) => {
                  e.stopPropagation();
                  onDelete(folder);
                }}
                className="text-[10px] grayscale hover:grayscale-0 transition-all"
              >
                ✕
              </button>
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
            className={`group relative flex flex-col items-center p-4 rounded-2xl cursor-default transition-all border hover:scale-105 ${
              isSelected
                ? 'bg-indigo-50 border-indigo-200 shadow-lg'
                : 'hover:bg-white hover:shadow-xl border-transparent'
            }`}
          >
            <div className="text-5xl mb-2 transition-transform duration-500 group-hover:-rotate-12">{getFileIcon(file)}</div>
            <span className="text-[11px] font-black text-slate-800 truncate w-full text-center px-1 tracking-tight" title={file.name}>
              {file.name}
            </span>
            <span className="text-[9px] text-slate-300 font-bold uppercase mt-1">
              {formatSize(file.size)}
            </span>

            {/* Hover Actions */}
            <div className="absolute -bottom-2 opacity-0 group-hover:opacity-100 transition-all flex space-x-2 bg-white px-3 py-1.5 rounded-2xl shadow-xl border border-slate-50 z-10">
              <button
                onClick={(e) => {
                  e.stopPropagation();
                  onRename(file);
                }}
                className="text-[10px] grayscale hover:grayscale-0 transition-all"
              >
                ✏️
              </button>
              <button
                onClick={(e) => {
                  e.stopPropagation();
                  onDelete(file);
                }}
                className="text-[10px] grayscale hover:grayscale-0 transition-all"
              >
                ✕
              </button>
            </div>
          </div>
        );
      })}
    </div>
  );
}
