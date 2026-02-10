import React from 'react';

interface FileItem {
  id: number;
  filename: string;
  size: number;
  upload_time: string;
  comment: string;
}

interface FileGridProps {
  currentPath: string[];
  filteredFolders: string[];
  filteredFiles: FileItem[];
  isCreating: boolean;
  newFolderName: string;
  dragOverFolder: string | null;
  setNewFolderName: (name: string) => void;
  onBack: () => void;
  onNavigate: (folder: string) => void;
  onDownload: (id: number, name: string) => void;
  onMove: (oldPath: string, newPath: string, isFolder: boolean) => void;
  onDelete: (path: string) => void;
  submitFolder: () => void;
  cancelFolder: () => void;
  handleDragStart: (e: React.DragEvent, path: string, isFolder: boolean) => void;
  handleDrop: (e: React.DragEvent, targetFolder: string) => void;
  setDragOverFolder: (folder: string | null) => void;
  openConfirm: (title: string, message: string, onOk: () => void) => void;
  openPrompt: (title: string, message: string, initialValue: string, onOk: (v: string) => void) => void;
  onContextMenu: (e: React.MouseEvent, type: 'file' | 'folder', data: any) => void;
  selectedFiles: number[];
  selectedFolders: string[];
  onFileSelect: (fileId: number, ctrlKey: boolean) => void;
}

export default function FileGrid({
  currentPath, filteredFolders, filteredFiles, isCreating, newFolderName, dragOverFolder,
  setNewFolderName, onBack, onNavigate, onDownload, onMove, onDelete,
  submitFolder, cancelFolder, handleDragStart, handleDrop, setDragOverFolder,
  openConfirm, openPrompt, onContextMenu, selectedFiles, selectedFolders, onFileSelect
}: FileGridProps) {
  return (
    <div className="grid grid-cols-2 sm:grid-cols-4 md:grid-cols-6 lg:grid-cols-8 xl:grid-cols-10 gap-8">
      {/* Back Button */}
      {currentPath.length > 0 && (
        <div
          onClick={onBack}
          onDragOver={(e) => e.preventDefault()}
          onDrop={(e) => {
            e.preventDefault();
            const sourcePath = e.dataTransfer.getData('sourcePath');
            const isFolder = e.dataTransfer.getData('isFolder') === 'true';
            const fileName = sourcePath.split('/').pop();
            const parentPath = currentPath.slice(0, -1).join('/');
            const targetPath = parentPath ? `${parentPath}/${fileName}` : fileName || '';
            if (sourcePath !== targetPath) onMove(sourcePath, targetPath, isFolder);
          }}
          className="group flex flex-col items-center p-4 rounded-[32px] hover:bg-white hover:shadow-xl hover:shadow-slate-200/50 cursor-pointer transition-all border border-transparent hover:border-slate-100"
        >
          <div className="text-6xl mb-3 opacity-50 group-hover:opacity-100 transform group-hover:-translate-y-1 transition-all">📁</div>
          <span className="text-[10px] font-black text-slate-500 uppercase tracking-tighter">..</span>
          <span className="text-[8px] text-slate-300 font-bold uppercase mt-1">上级目录</span>
        </div>
      )}

      {/* New Folder Placeholder */}
      {isCreating && (
        <div className="flex flex-col items-center p-4 rounded-[32px] bg-white shadow-2xl shadow-indigo-200 ring-2 ring-indigo-500 relative group/new">
          <div className="text-6xl mb-3 animate-pulse">📂</div>
          <input 
            autoFocus
            value={newFolderName}
            onChange={(e) => setNewFolderName(e.target.value)}
            onBlur={submitFolder}
            onKeyDown={(e) => {
              if (e.key === 'Enter') submitFolder();
              if (e.key === 'Escape') cancelFolder();
            }}
            placeholder="名称..."
            className="w-full bg-transparent text-center text-[10px] font-black outline-none text-indigo-700 uppercase tracking-tighter"
          />
          <div className="absolute -top-2 -right-2 opacity-0 group-hover/new:opacity-100 transition-opacity">
            <button onClick={cancelFolder} className="w-6 h-6 bg-white border border-slate-100 rounded-full flex items-center justify-center text-[8px] shadow-sm hover:bg-slate-50">✕</button>
          </div>
        </div>
      )}

      {/* Folders */}
      {filteredFolders.map(folder => {
        const fullPath = currentPath.length > 0 ? `${currentPath.join('/')}/${folder}` : folder;
        const isDragOver = dragOverFolder === folder;
        const isSelected = selectedFolders.includes(folder);

        return (
          <div
            key={folder}
            data-folder-name={folder}
            draggable
            onDragStart={(e) => handleDragStart(e, fullPath, true)}
            onDragOver={(e) => {
              e.preventDefault();
              setDragOverFolder(folder);
            }}
            onDragLeave={() => setDragOverFolder(null)}
            onDrop={(e) => {
              setDragOverFolder(null);
              handleDrop(e, folder);
            }}
            onContextMenu={(e) => onContextMenu(e, 'folder', folder)}
            className={`group relative flex flex-col items-center p-4 rounded-[32px] transition-all border-2 ${
              isDragOver 
                ? 'bg-indigo-50 border-indigo-400 scale-110 z-10 shadow-2xl' 
                : isSelected
                  ? 'bg-indigo-50/50 border-indigo-200 shadow-lg'
                  : 'hover:bg-white hover:shadow-2xl hover:shadow-slate-200/60 border-transparent hover:scale-105'
            } cursor-pointer`}
          >
            <div onClick={() => onNavigate(folder)} className="w-full flex flex-col items-center">
              <div className="text-6xl mb-3 transition-transform duration-500 group-hover:rotate-12 drop-shadow-sm">📂</div>
              <span className="text-[11px] font-black text-slate-700 truncate w-full text-center px-2 group-hover:text-indigo-700 uppercase tracking-tight">
                {folder}
              </span>
            </div>

            {/* Folder Actions */}
            <div className="absolute -bottom-2 opacity-0 group-hover:opacity-100 transition-all flex space-x-2 bg-white px-3 py-1.5 rounded-2xl shadow-xl border border-slate-50 z-10">
              <button
                onClick={(e) => {
                  e.stopPropagation();
                  openPrompt('修改文件夹路径', '请输入新的完整路径:', fullPath, (newPath) => {
                    if (newPath && newPath !== fullPath) onMove(fullPath, newPath, true);
                  });
                }}
                className="text-[10px] grayscale hover:grayscale-0 transition-all"
              >
                ✏️
              </button>
              <button
                onClick={(e) => {
                  e.stopPropagation();
                  openConfirm('删除文件夹', `确定要删除文件夹 "${folder}" 及其所有内容吗？`, () => {
                    onDelete(fullPath);
                  });
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
      {filteredFiles.map(file => {
        const isSelected = selectedFiles.includes(file.id);
        return (
          <div
            key={file.id}
            data-file-id={file.id}
            draggable
            onDragStart={(e) => handleDragStart(e, file.filename, false)}
            onContextMenu={(e) => onContextMenu(e, 'file', file)}
            onClick={(e) => onFileSelect(file.id, e.ctrlKey)}
            className={`group relative flex flex-col items-center p-4 rounded-[32px] cursor-default transition-all border hover:scale-105 ${
              isSelected 
                ? 'bg-indigo-50 border-indigo-200 shadow-lg shadow-indigo-100/50' 
                : 'hover:bg-white hover:shadow-2xl hover:shadow-indigo-100/40 border-transparent'
            }`}
          >
          <div className="text-6xl mb-3 transition-transform duration-500 group-hover:-rotate-12 drop-shadow-sm">📄</div>
          <span className="text-[11px] font-black text-slate-800 truncate w-full text-center px-1 uppercase tracking-tight" title={file.filename}>
            {file.filename.split('/').pop()}
          </span>
          <span className="text-[9px] text-slate-300 font-black mt-2 uppercase tracking-widest">
            {(file.size / 1024).toFixed(0)} KB
          </span>

          {/* Side Actions (Main) */}
          <button
            onClick={() => onDownload(file.id, file.filename)}
            className="absolute top-2 right-2 opacity-0 group-hover:opacity-100 transition-all bg-indigo-600 text-white w-8 h-8 rounded-xl flex items-center justify-center hover:scale-110 shadow-lg shadow-indigo-200"
          >
            <span className="text-xs">⬇</span>
          </button>

          {/* Bottom Actions (Extra) */}
          <div className="absolute -bottom-2 opacity-0 group-hover:opacity-100 transition-all flex space-x-2 bg-white px-3 py-1.5 rounded-2xl shadow-xl border border-slate-50">
            <button
              onClick={() => {
                const currentName = file.filename.split('/').pop() || '';
                openPrompt('重命名/移动文件', '请输入新的文件名或路径:', currentName, (newName) => {
                  if (newName && newName !== currentName) {
                    const prefix = currentPath.length > 0 ? `${currentPath.join('/')}/` : '';
                    onMove(file.filename, prefix + newName, false);
                  }
                });
              }}
              className="text-[10px] grayscale hover:grayscale-0 transition-all"
            >
              ✏️
            </button>
            <button
              onClick={() => {
                openConfirm('删除文件', `确定要删除文件 "${file.filename}" 吗？`, () => {
                  onDelete(file.filename);
                });
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
