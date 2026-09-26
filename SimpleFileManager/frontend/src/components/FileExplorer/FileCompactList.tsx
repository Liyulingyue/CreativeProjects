import type { MouseEvent, DragEvent } from 'react';
import type { FileNode } from '../../api';
import { formatSize, formatDate } from './utils';
import { FileIcon, Icon } from '../ui/Icon';

interface FileCompactListProps {
  items: FileNode[];
  selectedPaths: Set<string>;
  onSelect: (node: FileNode, e: MouseEvent) => void;
  onDoubleClick: (node: FileNode) => void;
  onContextMenu: (e: MouseEvent, node: FileNode) => void;
  onDragStart: (e: DragEvent, node: FileNode) => void;
  onDrop: (e: DragEvent, targetFolder: string) => void;
  onRename: (node: FileNode) => void;
  onDelete: (node: FileNode) => void;
  onDownload: (node: FileNode) => void;
  dragOverFolder: string | null;
  setDragOverFolder: (folder: string | null) => void;
  onBack: () => void;
  hasParent: boolean;
}

export default function FileCompactList({
  items, selectedPaths, onSelect, onDoubleClick, onContextMenu,
  onDragStart, onDrop, onRename, onDelete, onDownload, dragOverFolder, setDragOverFolder,
  onBack, hasParent
}: FileCompactListProps) {
  const folders = items.filter(i => i.is_dir);
  const files = items.filter(i => !i.is_dir);
  const hoverBtn = 'p-1 rounded text-slate-400 hover:text-slate-700 hover:bg-slate-100 transition-colors';
  const hoverBtnDanger = 'p-1 rounded text-slate-400 hover:text-red-600 hover:bg-red-50 transition-colors';

  return (
    <div className="flex flex-col p-2 gap-0.5">
      {/* Header */}
      <div className="flex items-center px-2 py-1.5 text-[10px] font-medium text-slate-400 uppercase tracking-wider border-b border-slate-200 mb-0.5">
        <div className="w-7"></div>
        <div className="flex-1">名称</div>
        <div className="w-14 text-right hidden sm:block">大小</div>
        <div className="w-20 text-right hidden md:block">修改日期</div>
        <div className="w-16 text-right pr-1 hidden sm:block">操作</div>
      </div>

      {/* Back */}
      {hasParent && (
        <div
          onClick={onBack}
          onDragOver={(e) => e.preventDefault()}
          className="flex items-center px-2 py-1.5 rounded-md hover:bg-slate-100 cursor-pointer transition-colors group"
        >
          <div className="w-7 flex items-center"><Icon name="arrowLeft" size={14} className="text-slate-400" /></div>
          <div className="flex-1 text-xs text-slate-500">..</div>
        </div>
      )}

      {/* Folders */}
      {folders.map(folder => {
        const isDragOver = dragOverFolder === folder.path;
        const isSelected = selectedPaths.has(folder.path);

        return (
          <div
            key={folder.path}
            draggable
            onDragStart={(e) => onDragStart(e, folder)}
            onDragOver={(e) => { e.preventDefault(); setDragOverFolder(folder.path); }}
            onDragLeave={() => setDragOverFolder(null)}
            onDrop={(e) => { setDragOverFolder(null); onDrop(e, folder.path); }}
            onClick={(e) => onSelect(folder, e)}
            onDoubleClick={() => onDoubleClick(folder)}
            onContextMenu={(e) => onContextMenu(e, folder)}
            className={`group flex items-center px-2 py-1.5 rounded-md transition-colors border cursor-pointer ${
              isDragOver
                ? 'bg-indigo-50 border-indigo-300'
                : isSelected
                  ? 'bg-indigo-50 border-transparent'
                  : 'border-transparent hover:bg-slate-50'
            }`}
          >
            <div className="w-7 flex items-center"><FileIcon node={folder} size={16} /></div>
            <div className="flex-1 text-xs text-slate-700 truncate min-w-0">{folder.name}</div>
            <div className="w-14 text-[10px] text-slate-400 text-right hidden sm:block">—</div>
            <div className="w-20 text-[10px] text-slate-400 text-right hidden md:block">{formatDate(folder.modified)}</div>
            <div className="w-16 flex justify-end gap-0.5 opacity-0 group-hover:opacity-100 transition-opacity hidden sm:flex">
              <button onClick={(e) => { e.stopPropagation(); onRename(folder); }} className={hoverBtn}><Icon name="edit" size={12} /></button>
              <button onClick={(e) => { e.stopPropagation(); onDelete(folder); }} className={hoverBtnDanger}><Icon name="trash" size={12} /></button>
            </div>
          </div>
        );
      })}

      {/* Files */}
      {files.map(file => {
        const isSelected = selectedPaths.has(file.path);

        return (
          <div
            key={file.path}
            draggable
            onDragStart={(e) => onDragStart(e, file)}
            onClick={(e) => onSelect(file, e)}
            onDoubleClick={() => onDoubleClick(file)}
            onContextMenu={(e) => onContextMenu(e, file)}
            className={`group flex items-center px-2 py-1.5 rounded-md transition-colors border ${
              isSelected
                ? 'bg-indigo-50 border-transparent'
                : 'border-transparent hover:bg-slate-50'
            }`}
          >
            <div className="w-7 flex items-center"><FileIcon node={file} size={16} /></div>
            <div className="flex-1 text-xs text-slate-800 truncate min-w-0">{file.name}</div>
            <div className="w-14 text-[10px] text-slate-500 text-right hidden sm:block">{formatSize(file.size)}</div>
            <div className="w-20 text-[10px] text-slate-400 text-right hidden md:block">{formatDate(file.modified)}</div>
            <div className="w-16 flex justify-end gap-0.5 opacity-0 group-hover:opacity-100 transition-opacity hidden sm:flex">
              <button onClick={(e) => { e.stopPropagation(); onDownload(file); }} className={hoverBtn}><Icon name="download" size={12} /></button>
              <button onClick={(e) => { e.stopPropagation(); onRename(file); }} className={hoverBtn}><Icon name="edit" size={12} /></button>
              <button onClick={(e) => { e.stopPropagation(); onDelete(file); }} className={hoverBtnDanger}><Icon name="trash" size={12} /></button>
            </div>
          </div>
        );
      })}
    </div>
  );
}
