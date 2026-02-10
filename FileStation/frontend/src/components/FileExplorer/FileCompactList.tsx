import React from 'react';

interface FileItem {
  id: number;
  filename: string;
  size: number;
  upload_time: string;
  comment: string;
}

interface FileCompactListProps {
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

export default function FileCompactList({
  currentPath, filteredFolders, filteredFiles, isCreating, newFolderName, dragOverFolder,
  setNewFolderName, onBack, onNavigate, onDownload, onMove, onDelete,
  submitFolder, cancelFolder, handleDragStart, handleDrop, setDragOverFolder,
  openConfirm, openPrompt
}: FileCompactListProps) {
  return (
    <div className="flex flex-col space-y-1">
      {/* Compact View Header */}
      <div className="flex items-center px-4 py-1.5 bg-slate-50/50 rounded-lg text-[8px] font-bold text-slate-400 uppercase tracking-tight border border-slate-100/50 mb-1">
        <div className="w-8"></div>
        <div className="flex-1">名称</div>
        <div className="w-16 text-center">类型</div>
        <div className="w-16 text-center">大小</div>
        <div className="w-24 text-center">修改日期</div>
        <div className="w-20 text-right pr-2">操作</div>
      </div>

      {/* Back Button for Compact List Mode */}
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
          className="group flex items-center px-4 py-2 transition-all border bg-slate-50/50 border-slate-100 hover:shadow-sm hover:shadow-slate-200/40 rounded-lg cursor-pointer hover:bg-slate-100"
        >
          <div className="w-8 flex items-center justify-center">
            <span className="text-base group-hover:scale-110 transition-transform">⬆</span>
          </div>
          <div className="flex-1 text-[10px] font-bold text-slate-600 uppercase tracking-tight">..</div>
          <div className="w-16 text-[8px] text-slate-400 font-medium uppercase text-center">BACK</div>
          <div className="w-16 text-[8px] text-slate-300 font-medium text-center">--</div>
          <div className="w-24 text-[8px] text-slate-300 font-medium text-center">--</div>
          <div className="w-20 flex justify-end space-x-1 opacity-0 group-hover:opacity-100 transition-opacity">
            <span className="text-[7px] text-slate-300 mr-1">返回</span>
          </div>
        </div>
      )}

      {/* New Folder Placeholder for Compact List Mode */}
      {isCreating && (
        <div className="flex items-center px-4 py-2 bg-white shadow-lg shadow-indigo-200 ring-1 ring-indigo-500 rounded-lg border border-indigo-100 group/new">
          <div className="w-8 flex items-center justify-center">
            <span className="text-base animate-pulse">📂</span>
          </div>
          <div className="flex-1">
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
              className="bg-transparent text-[10px] font-bold outline-none text-indigo-700 uppercase tracking-tight w-full"
            />
          </div>
          <div className="w-16 text-[8px] text-slate-400 font-medium uppercase text-center">FOLDER</div>
          <div className="w-16 text-[8px] text-slate-300 font-medium text-center">--</div>
          <div className="w-24 text-[8px] text-slate-300 font-medium text-center">--</div>
          <div className="w-20 flex justify-end">
            <button onClick={cancelFolder} className="text-[7px] text-slate-300 hover:text-red-500 mr-1">放弃</button>
          </div>
        </div>
      )}

      {/* Compact Folders */}
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
            className={`group flex items-center px-4 py-2 transition-all border ${
              isDragOver
                ? 'bg-indigo-50 border-indigo-400 translate-x-1'
                : 'bg-white border-slate-50 hover:shadow-sm hover:shadow-slate-200/40'
            } rounded-lg cursor-pointer`}
          >
            <div className="w-8 flex items-center justify-center">
              <span className="text-base group-hover:scale-110 transition-transform">📂</span>
            </div>
            <div className="flex-1 text-[10px] font-bold text-slate-700 uppercase tracking-tight truncate">{folder}</div>
            <div className="w-16 text-[8px] text-slate-400 font-medium uppercase text-center">FOLDER</div>
            <div className="w-16 text-[8px] text-slate-300 font-medium text-center">--</div>
            <div className="w-24 text-[8px] text-slate-300 font-medium text-center">--</div>
            <div className="w-20 flex justify-end space-x-1 opacity-0 group-hover:opacity-100 transition-opacity">
              <button
                onClick={(e) => {
                  e.stopPropagation();
                  openPrompt('修改文件夹路径', '请输入新的完整路径:', fullPath, (newPath) => {
                    if (newPath && newPath !== fullPath) onMove(fullPath, newPath, true);
                  });
                }}
                className="w-5 h-5 flex items-center justify-center rounded bg-slate-100 text-slate-600 hover:bg-slate-200 active:scale-90 transition-all text-[7px]"
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
                className="w-5 h-5 flex items-center justify-center rounded bg-red-100 text-red-600 hover:bg-red-200 active:scale-90 transition-all text-[7px]"
              >
                ✕
              </button>
            </div>
          </div>
        );
      })}

      {/* Compact Files */}
      {filteredFiles.map(file => {
        const fileName = file.filename.split('/').pop() || '';
        const fileExt = fileName.split('.').pop()?.toUpperCase() || 'FILE';
        const fileSize = (file.size / 1024).toFixed(0) + ' KB';

        return (
          <div
            key={file.id}
            draggable
            onDragStart={(e) => handleDragStart(e, file.filename, false)}
            className="group flex items-center px-4 py-2 bg-white rounded-lg hover:shadow-sm hover:shadow-indigo-100/30 transition-all border border-slate-50"
          >
            <div className="w-8 flex items-center justify-center">
              <span className="text-base group-hover:scale-110 transition-transform">📄</span>
            </div>
            <div className="flex-1 text-[10px] font-bold text-slate-800 uppercase tracking-tight truncate">{fileName}</div>
            <div className="w-16 text-[8px] text-slate-400 font-medium uppercase text-center">{fileExt}</div>
            <div className="w-16 text-[8px] text-slate-500 font-medium text-center">{fileSize}</div>
            <div className="w-24 text-[8px] text-slate-400 font-medium text-center">{new Date(file.upload_time).toLocaleDateString()}</div>
            <div className="w-20 flex justify-end space-x-1 opacity-0 group-hover:opacity-100 transition-opacity">
              <button onClick={() => onDownload(file.id, file.filename)} className="w-5 h-5 flex items-center justify-center rounded bg-indigo-600 text-white shadow-sm shadow-indigo-100 hover:bg-indigo-700 active:scale-90 transition-all text-[7px]">⬇</button>
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
                className="w-5 h-5 flex items-center justify-center rounded bg-slate-100 text-slate-600 hover:bg-slate-200 active:scale-90 transition-all text-[7px]"
              >
                ✏️
              </button>
              <button
                onClick={() => {
                  openConfirm('删除文件', `确定要删除文件 "${file.filename}" 吗？`, () => {
                    onDelete(file.filename);
                  });
                }}
                className="w-5 h-5 flex items-center justify-center rounded bg-red-100 text-red-600 hover:bg-red-200 active:scale-90 transition-all text-[7px]"
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