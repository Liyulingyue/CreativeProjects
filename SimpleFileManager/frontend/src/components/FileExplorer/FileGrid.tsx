import type { MouseEvent, DragEvent } from 'react';
import type { FileNode } from '../../api';
import { formatSize } from './utils';
import { FileIcon, Icon } from '../ui/Icon';

interface FileGridProps {
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
  isCreatingFolder: boolean;
  newFolderName: string;
  setNewFolderName: (name: string) => void;
  onCreateFolder: () => void;
  onCancelCreateFolder: () => void;
  onBack: () => void;
  hasParent: boolean;
}

export default function FileGrid({
  items, selectedPaths, onSelect, onDoubleClick, onContextMenu,
  onDragStart, onDrop, onRename, onDelete, onDownload, dragOverFolder, setDragOverFolder,
  isCreatingFolder, newFolderName, setNewFolderName, onCreateFolder, onCancelCreateFolder,
  onBack, hasParent
}: FileGridProps) {
  const folders = items.filter(i => i.is_dir);
  const files = items.filter(i => !i.is_dir);

  const hoverBtn = 'p-1.5 rounded-md text-slate-400 hover:text-slate-700 hover:bg-slate-100 transition-colors';
  const hoverBtnDanger = 'p-1.5 rounded-md text-slate-400 hover:text-red-600 hover:bg-red-50 transition-colors';

  return (
    <div className="grid grid-cols-3 xs:grid-cols-4 sm:grid-cols-5 md:grid-cols-6 lg:grid-cols-8 xl:grid-cols-10 gap-2 sm:gap-3 p-3 sm:p-4">
      {/* Back */}
      {hasParent && (
        <div
          onClick={onBack}
          onDragOver={(e) => e.preventDefault()}
          className="flex flex-col items-center p-3 rounded-xl hover:bg-white hover:shadow-card cursor-pointer transition-all border border-transparent hover:border-slate-200"
        >
          <div className="mb-1.5 opacity-60"><Icon name="folder" size={40} className="text-amber-500" /></div>
          <span className="text-xs text-slate-500 truncate w-full text-center">..</span>
        </div>
      )}

      {/* New Folder Input */}
      {isCreatingFolder && (
        <div className="flex flex-col items-center p-3 rounded-xl bg-white shadow-card ring-2 ring-indigo-400 relative">
          <div className="mb-1.5"><Icon name="folder" size={40} className="text-amber-500" /></div>
          <input
            autoFocus
            value={newFolderName}
            onChange={(e) => setNewFolderName(e.target.value)}
            onBlur={onCreateFolder}
            onKeyDown={(e) => {
              if (e.key === 'Enter') onCreateFolder();
              if (e.key === 'Escape') onCancelCreateFolder();
            }}
            placeholder="文件夹名称"
            className="w-full bg-transparent text-center text-xs text-slate-700 outline-none border-b border-indigo-300 pb-0.5"
          />
          <button
            onClick={onCancelCreateFolder}
            className="absolute top-1.5 right-1.5 p-1 rounded-md text-slate-400 hover:text-slate-700 hover:bg-slate-100"
          >
            <Icon name="x" size={14} />
          </button>
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
            className={`group relative flex flex-col items-center p-3 rounded-xl transition-all border cursor-pointer ${
              isDragOver
                ? 'bg-indigo-50 border-indigo-400 scale-105 z-10 shadow-popover'
                : isSelected
                  ? 'bg-indigo-50 border-indigo-300 shadow-card'
                  : 'border-transparent hover:bg-white hover:shadow-card hover:border-slate-200'
            }`}
          >
            <div className="mb-1.5 transition-transform duration-200 group-hover:scale-110">
              <FileIcon node={folder} size={40} />
            </div>
            <span className="text-xs text-slate-700 truncate w-full text-center px-1" title={folder.name}>
              {folder.name}
            </span>

            <div className="absolute -top-2 right-1 opacity-0 group-hover:opacity-100 transition-opacity flex items-center gap-0.5 bg-white rounded-lg shadow-popover border border-slate-100 px-1 py-0.5 z-10">
              <button onClick={(e) => { e.stopPropagation(); onRename(folder); }} className={hoverBtn}><Icon name="edit" size={14} /></button>
              <button onClick={(e) => { e.stopPropagation(); onDelete(folder); }} className={hoverBtnDanger}><Icon name="trash" size={14} /></button>
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
            className={`group relative flex flex-col items-center p-3 rounded-xl transition-all border cursor-default ${
              isSelected
                ? 'bg-indigo-50 border-indigo-300 shadow-card'
                : 'border-transparent hover:bg-white hover:shadow-card hover:border-slate-200'
            }`}
          >
            <div className="mb-1.5 transition-transform duration-200 group-hover:scale-110">
              <FileIcon node={file} size={40} />
            </div>
            <span className="text-xs text-slate-800 truncate w-full text-center px-1" title={file.name}>
              {file.name}
            </span>
            <span className="text-[10px] text-slate-400 mt-0.5">{formatSize(file.size)}</span>

            <div className="absolute -top-2 right-1 opacity-0 group-hover:opacity-100 transition-opacity flex items-center gap-0.5 bg-white rounded-lg shadow-popover border border-slate-100 px-1 py-0.5 z-10">
              <button onClick={(e) => { e.stopPropagation(); onDownload(file); }} className={hoverBtn}><Icon name="download" size={14} /></button>
              <button onClick={(e) => { e.stopPropagation(); onRename(file); }} className={hoverBtn}><Icon name="edit" size={14} /></button>
              <button onClick={(e) => { e.stopPropagation(); onDelete(file); }} className={hoverBtnDanger}><Icon name="trash" size={14} /></button>
            </div>
          </div>
        );
      })}
    </div>
  );
}
