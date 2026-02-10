import React from 'react';

interface FileItem {
  id: number;
  filename: string;
  size: number;
  upload_time: string;
  comment: string;
}

interface FileListProps {
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
}

export default function FileList({
  currentPath, filteredFolders, filteredFiles, isCreating, newFolderName, dragOverFolder,
  setNewFolderName, onBack, onNavigate, onDownload, onMove, onDelete,
  submitFolder, cancelFolder, handleDragStart, handleDrop, setDragOverFolder,
  openConfirm, openPrompt
}: FileListProps) {
  return (
    <div className="flex flex-col space-y-2">
      {/* List View Header */}
      <div className="flex items-center px-8 py-3 bg-white/50 rounded-2xl text-[10px] font-black text-slate-400 uppercase tracking-widest border border-slate-100">
        <div className="flex-1">名称</div>
        <div className="w-32">大小</div>
        <div className="w-48 text-right">快捷操作</div>
      </div>

      {/* Back Button for List Mode */}
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
          className="group flex items-center px-8 py-4 transition-all border bg-slate-50/50 border-slate-100 hover:shadow-xl hover:shadow-slate-200/40 rounded-[24px] cursor-pointer hover:bg-slate-100"
        >
          <div className="flex-1 flex items-center">
            <span className="text-3xl mr-4 group-hover:scale-110 transition-transform">⬆</span>
            <div>
              <div className="text-xs font-black text-slate-600 uppercase tracking-tight">..</div>
              <div className="text-[9px] text-slate-400 font-bold uppercase mt-0.5">上级目录</div>
            </div>
          </div>
          <div className="w-32 text-[10px] font-black text-slate-400">--</div>
          <div className="w-48 flex justify-end space-x-2 opacity-0 group-hover:opacity-100 transition-opacity">
            <span className="text-[10px] text-slate-300">返回</span>
          </div>
        </div>
      )}

      {/* New Folder Placeholder for List Mode */}
      {isCreating && (
        <div className="flex items-center px-8 py-4 bg-white shadow-2xl shadow-indigo-200 ring-2 ring-indigo-500 rounded-[24px] border border-indigo-100 group/new">
          <div className="flex-1 flex items-center">
            <span className="text-3xl mr-4 animate-pulse">📂</span>
            <div>
              <input 
                autoFocus
                value={newFolderName}
                onChange={(e) => setNewFolderName(e.target.value)}
                onBlur={submitFolder}
                onKeyDown={(e) => {
                  if (e.key === 'Enter') submitFolder();
                  if (e.key === 'Escape') cancelFolder();
                }}
                placeholder="文件夹名称..."
                className="bg-transparent text-xs font-black outline-none text-indigo-700 uppercase tracking-tight"
              />
              <div className="text-[9px] text-slate-300 font-bold uppercase mt-0.5">按 ESC 取消</div>
            </div>
          </div>
          <div className="w-32 text-[10px] font-black text-slate-400">--</div>
          <div className="w-48 flex justify-end space-x-2">
            <button onClick={cancelFolder} className="text-[10px] text-slate-300 hover:text-red-500">放弃创建</button>
          </div>
        </div>
      )}

      {/* List Folders */}
      {filteredFolders.map(folder => {
        const fullPath = currentPath.length > 0 ? `${currentPath.join('/')}/${folder}` : folder;
        const isDragOver = dragOverFolder === folder;

        return (
          <div
            key={folder}
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
            onClick={() => onNavigate(folder)}
            className={`group flex items-center px-8 py-4 transition-all border ${
              isDragOver 
                ? 'bg-indigo-50 border-indigo-400 translate-x-2' 
                : 'bg-white border-slate-50 hover:shadow-xl hover:shadow-slate-200/40'
            } rounded-[24px] cursor-pointer`}
          >
            <div className="flex-1 flex items-center">
              <span className="text-3xl mr-4 group-hover:scale-110 transition-transform">📂</span>
              <div>
                <div className="text-xs font-black text-slate-700 uppercase tracking-tight">{folder}</div>
                <div className="text-[9px] text-slate-300 font-bold uppercase mt-0.5">文件夹</div>
              </div>
            </div>
            <div className="w-32 text-[10px] font-black text-slate-400">--</div>
            <div className="w-48 flex justify-end space-x-3">
              <button 
                onClick={(e) => { 
                  e.stopPropagation(); 
                  openPrompt('修改文件夹路径', '请输入新的完整路径:', fullPath, (newPath) => {
                    if (newPath && newPath !== fullPath) onMove(fullPath, newPath, true);
                  });
                }} 
                className="w-9 h-9 flex items-center justify-center rounded-xl bg-slate-100 text-slate-600 hover:bg-slate-200 active:scale-90 transition-all"
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
                className="w-9 h-9 flex items-center justify-center rounded-xl bg-red-100 text-red-600 hover:bg-red-200 active:scale-90 transition-all"
              >
                ✕
              </button>
            </div>
          </div>
        );
      })}

      {/* List Files */}
      {filteredFiles.map(file => (
        <div
          key={file.id}
          draggable
          onDragStart={(e) => handleDragStart(e, file.filename, false)}
          className="group flex items-center px-8 py-4 bg-white rounded-[24px] hover:shadow-xl hover:shadow-indigo-100/30 transition-all border border-slate-50"
        >
          <div className="flex-1 flex items-center">
            <span className="text-3xl mr-4 group-hover:scale-110 transition-transform">📄</span>
            <div>
              <div className="text-xs font-black text-slate-800 uppercase tracking-tight">{file.filename.split('/').pop()}</div>
              <div className="text-[9px] text-slate-300 font-bold uppercase mt-0.5">{new Date(file.upload_time).toLocaleDateString()}</div>
            </div>
          </div>
          <div className="w-32 text-[10px] font-black text-slate-500">{(file.size / 1024).toFixed(0)} KB</div>
          <div className="w-48 flex justify-end space-x-3">
            <button onClick={() => onDownload(file.id, file.filename)} className="w-9 h-9 flex items-center justify-center rounded-xl bg-indigo-600 text-white shadow-lg shadow-indigo-100 hover:bg-indigo-700 active:scale-90 transition-all">⬇</button>
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
              className="w-9 h-9 flex items-center justify-center rounded-xl bg-slate-100 text-slate-600 hover:bg-slate-200 active:scale-90 transition-all"
            >
              ✏️
            </button>
            <button 
              onClick={() => {
                openConfirm('删除文件', `确定要删除文件 "${file.filename}" 吗？`, () => {
                  onDelete(file.filename);
                });
              }}
              className="w-9 h-9 flex items-center justify-center rounded-xl bg-red-100 text-red-600 hover:bg-red-200 active:scale-90 transition-all"
            >
              ✕
            </button>
          </div>
        </div>
      ))}
    </div>
  );
}
