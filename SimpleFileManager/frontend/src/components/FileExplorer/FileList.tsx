import type { MouseEvent, DragEvent } from 'react';
import type { FileNode } from '../../api';

interface FileListProps {
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

export default function FileList({
  items, selectedPath, onSelect, onDoubleClick, onContextMenu,
  onDragStart, onDrop, onRename, onDelete, dragOverFolder, setDragOverFolder,
  onBack, hasParent
}: FileListProps) {
  const folders = items.filter(i => i.is_dir);
  const files = items.filter(i => !i.is_dir);

  return (
    <div className="flex flex-col space-y-2 p-4">
      {/* List View Header */}
      <div className="flex items-center px-8 py-3 bg-white/50 rounded-xl text-[10px] font-black text-slate-400 uppercase tracking-widest border border-slate-100">
        <div className="flex-1">名称</div>
        <div className="w-24">类型</div>
        <div className="w-32">大小</div>
        <div className="w-40">修改时间</div>
        <div className="w-48 text-right">操作</div>
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
          className="group flex items-center px-8 py-4 transition-all border bg-slate-50/50 border-slate-100 hover:shadow-xl rounded-xl cursor-pointer hover:bg-slate-100"
        >
          <div className="flex-1 flex items-center">
            <span className="text-3xl mr-4 group-hover:scale-110 transition-transform">⬆</span>
            <div>
              <div className="text-xs font-black text-slate-600 uppercase">..</div>
              <div className="text-[9px] text-slate-400 font-bold uppercase">上级目录</div>
            </div>
          </div>
          <div className="w-24 text-[10px] font-black text-slate-400">--</div>
          <div className="w-32 text-[10px] font-black text-slate-400">--</div>
          <div className="w-40 text-[10px] font-black text-slate-400">--</div>
          <div className="w-48 flex justify-end">
            <span className="text-[10px] text-slate-300">返回</span>
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
            className={`group flex items-center px-8 py-4 transition-all border rounded-xl cursor-pointer ${
              isDragOver
                ? 'bg-indigo-50 border-indigo-400 translate-x-2'
                : isSelected
                  ? 'bg-indigo-50 border-indigo-200 shadow-lg'
                  : 'bg-white border-slate-50 hover:shadow-xl'
            }`}
          >
            <div className="flex-1 flex items-center">
              <span className="text-3xl mr-4 group-hover:scale-110 transition-transform">{getFileIcon(folder)}</span>
              <span className="text-xs font-black text-slate-700">{folder.name}</span>
            </div>
            <div className="w-24 text-[10px] font-black text-slate-500 uppercase">FOLDER</div>
            <div className="w-32 text-[10px] font-black text-slate-400">--</div>
            <div className="w-40 text-[10px] font-black text-slate-400">{formatDate(folder.modified)}</div>
            <div className="w-48 flex justify-end space-x-3">
              <button
                onClick={(e) => { e.stopPropagation(); onRename(folder); }}
                className="w-9 h-9 flex items-center justify-center rounded-xl bg-slate-100 text-slate-600 hover:bg-slate-200 active:scale-90 transition-all"
              >✏️</button>
              <button
                onClick={(e) => { e.stopPropagation(); onDelete(folder); }}
                className="w-9 h-9 flex items-center justify-center rounded-xl bg-red-100 text-red-600 hover:bg-red-200 active:scale-90 transition-all"
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
            className={`group flex items-center px-8 py-4 rounded-xl transition-all border ${
              isSelected
                ? 'bg-indigo-50 border-indigo-200 shadow-lg'
                : 'bg-white border-slate-50 hover:shadow-xl'
            }`}
          >
            <div className="flex-1 flex items-center">
              <span className="text-3xl mr-4 group-hover:scale-110 transition-transform">{getFileIcon(file)}</span>
              <span className="text-xs font-black text-slate-800">{file.name}</span>
            </div>
            <div className="w-24 text-[10px] font-black text-slate-500 uppercase">{file.extension.slice(1) || 'FILE'}</div>
            <div className="w-32 text-[10px] font-black text-slate-500">{formatSize(file.size)}</div>
            <div className="w-40 text-[10px] font-black text-slate-500">{formatDate(file.modified)}</div>
            <div className="w-48 flex justify-end space-x-3">
              <button className="w-9 h-9 flex items-center justify-center rounded-xl bg-indigo-600 text-white shadow-lg hover:bg-indigo-700 active:scale-90 transition-all">⬇</button>
              <button
                onClick={(e) => { e.stopPropagation(); onRename(file); }}
                className="w-9 h-9 flex items-center justify-center rounded-xl bg-slate-100 text-slate-600 hover:bg-slate-200 active:scale-90 transition-all"
              >✏️</button>
              <button
                onClick={(e) => { e.stopPropagation(); onDelete(file); }}
                className="w-9 h-9 flex items-center justify-center rounded-xl bg-red-100 text-red-600 hover:bg-red-200 active:scale-90 transition-all"
              >✕</button>
            </div>
          </div>
        );
      })}
    </div>
  );
}
