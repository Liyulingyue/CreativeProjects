import { useState } from 'react';

interface FileItem {
  id: number;
  filename: string;
  size: number;
  upload_time: string;
  comment: string;
}

interface FileExplorerProps {
  subFolders: string[];
  currentFiles: FileItem[];
  currentPath: string[];
  onNavigate: (folder: string) => void;
  onBack: () => void;
  onRoot: () => void;
  onDownload: (id: number, name: string) => void;
  onCreateFolder: (name: string) => void;
  onUploadClick: () => void;
  onDelete: (filename: string) => void;
  onMove: (oldPath: string, newPath: string, isFolder: boolean) => void;
}

export default function FileExplorer({ 
  subFolders, currentFiles, currentPath, 
  onNavigate, onBack, onRoot, onDownload,
  onCreateFolder, onUploadClick,
  onDelete, onMove
}: FileExplorerProps) {
  const [isCreating, setIsCreating] = useState(false);
  const [newFolderName, setNewFolderName] = useState('');
  const [viewMode, setViewMode] = useState<'grid' | 'list'>('grid');
  const [searchQuery, setSearchQuery] = useState('');

  const filteredFolders = subFolders.filter(f => f.toLowerCase().includes(searchQuery.toLowerCase()));
  const filteredFiles = currentFiles.filter(f => f.filename.toLowerCase().includes(searchQuery.toLowerCase()));

  const handleDragStart = (e: React.DragEvent, path: string, isFolder: boolean) => {
    e.dataTransfer.setData('sourcePath', path);
    e.dataTransfer.setData('isFolder', String(isFolder));
    e.dataTransfer.effectAllowed = 'move';
  };

  const handleDrop = (e: React.DragEvent, targetFolder: string) => {
    e.preventDefault();
    const sourcePath = e.dataTransfer.getData('sourcePath');
    const isFolder = e.dataTransfer.getData('isFolder') === 'true';
    
    // Construct target path
    const targetPath = currentPath.length > 0 
      ? `${currentPath.join('/')}/${targetFolder}/${sourcePath.split('/').pop()}`
      : `${targetFolder}/${sourcePath.split('/').pop()}`;

    if (sourcePath !== targetPath) {
      onMove(sourcePath, targetPath, isFolder);
    }
  };

  const submitFolder = () => {
    if (newFolderName.trim()) {
      onCreateFolder(newFolderName.trim());
      setNewFolderName('');
      setIsCreating(false);
    }
  };

  return (
    <div className="flex-1 flex flex-col min-h-0 bg-white">
      {/* Dynamic Toolbar */}
      <div className="h-20 border-b border-slate-100 flex items-center justify-between px-10 bg-white/80 backdrop-blur-md sticky top-0 z-20">
        <div className="flex items-center space-x-2 overflow-x-auto no-scrollbar">
          <button onClick={onRoot} className="w-10 h-10 flex items-center justify-center rounded-xl hover:bg-slate-100 transition-colors text-xl">🏠</button>
          <span className="text-slate-300 font-black">/</span>
          {currentPath.map((folder, idx) => (
            <div key={idx} className="flex items-center space-x-2 shrink-0">
              <button className="px-3 py-1.5 rounded-xl hover:bg-slate-100 text-sm font-black text-slate-700 transition-colors uppercase tracking-tight">
                {folder}
              </button>
              <span className="text-slate-300 font-black">/</span>
            </div>
          ))}
        </div>

        {/* Search Bar */}
        <div className="flex-1 max-w-md mx-8 group">
          <div className="relative flex items-center">
            <span className="absolute left-4 text-slate-300 group-focus-within:text-indigo-400 transition-colors">🔍</span>
            <input 
              type="text"
              placeholder="搜索当前目录..."
              value={searchQuery}
              onChange={(e) => setSearchQuery(e.target.value)}
              className="w-full pl-12 pr-4 py-2.5 bg-slate-50 border border-slate-100 rounded-2xl text-[11px] font-black uppercase tracking-tight focus:bg-white focus:border-indigo-300 focus:ring-4 focus:ring-indigo-50 outline-none transition-all"
            />
            {searchQuery && (
              <button 
                onClick={() => setSearchQuery('')}
                className="absolute right-3 w-6 h-6 flex items-center justify-center rounded-lg hover:bg-slate-200 text-slate-400 transition-colors"
              >
                ✕
              </button>
            )}
          </div>
        </div>

        <div className="flex items-center space-x-4">
          <div className="flex bg-slate-100 p-1 rounded-xl mr-2">
            <button 
              onClick={() => setViewMode('grid')}
              className={`px-3 py-1 rounded-lg text-[10px] font-black uppercase transition-all ${viewMode === 'grid' ? 'bg-white shadow-sm text-indigo-600' : 'text-slate-400'}`}
            >
              Grid
            </button>
            <button 
              onClick={() => setViewMode('list')}
              className={`px-3 py-1 rounded-lg text-[10px] font-black uppercase transition-all ${viewMode === 'list' ? 'bg-white shadow-sm text-indigo-600' : 'text-slate-400'}`}
            >
              List
            </button>
          </div>
          <button 
            onClick={() => setIsCreating(true)}
            className="flex items-center px-5 py-2.5 bg-indigo-50 text-indigo-700 rounded-2xl text-xs font-black uppercase tracking-widest hover:bg-indigo-100 transition-all active:scale-95"
          >
            <span className="mr-2 text-base">➕</span> 新文件夹
          </button>
          <button 
            onClick={onUploadClick}
            className="flex items-center px-6 py-3 bg-indigo-600 text-white rounded-2xl text-xs font-black uppercase tracking-widest hover:bg-indigo-700 shadow-xl shadow-indigo-200 transition-all active:scale-95 translate-y-[-2px] hover:translate-y-[-4px]"
          >
            <span className="mr-2 text-base">📤</span> 上传
          </button>
        </div>
      </div>

      {/* Main Content Area */}
      <div className="flex-1 overflow-y-auto p-10 custom-scrollbar bg-slate-50/30">
        {viewMode === 'grid' ? (
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
                className="group flex flex-col items-center p-4 rounded-[32px] hover:bg-white hover:shadow-xl hover:shadow-slate-200/50 cursor-pointer transition-all border border-transparent"
              >
                <div className="text-6xl mb-3 opacity-30 group-hover:opacity-100 transform group-hover:-translate-y-2 transition-all">🔙</div>
                <span className="text-[10px] font-black text-slate-400 uppercase tracking-tighter">返回上层</span>
              </div>
            )}

            {/* New Folder Placeholder */}
            {isCreating && (
              <div className="flex flex-col items-center p-4 rounded-[32px] bg-white shadow-2xl shadow-indigo-200 ring-2 ring-indigo-500">
                <div className="text-6xl mb-3 animate-pulse">📂</div>
                <input 
                  autoFocus
                  value={newFolderName}
                  onChange={(e) => setNewFolderName(e.target.value)}
                  onBlur={submitFolder}
                  onKeyDown={(e) => e.key === 'Enter' && submitFolder()}
                  placeholder="名称..."
                  className="w-full bg-transparent text-center text-[10px] font-black outline-none text-indigo-700 uppercase tracking-tighter"
                />
              </div>
            )}

            {/* Folders */}
            {filteredFolders.map(folder => {
              const fullPath = currentPath.length > 0 ? `${currentPath.join('/')}/${folder}` : folder;
              return (
                <div
                  key={folder}
                  draggable
                  onDragStart={(e) => handleDragStart(e, fullPath, true)}
                  onDragOver={(e) => e.preventDefault()}
                  onDrop={(e) => handleDrop(e, folder)}
                  className="group relative flex flex-col items-center p-4 rounded-[32px] hover:bg-white hover:shadow-2xl hover:shadow-slate-200/60 cursor-pointer transition-all border border-transparent hover:scale-105"
                >
                  <div onClick={() => onNavigate(folder)} className="w-full flex flex-col items-center">
                    <div className="text-6xl mb-3 transition-transform duration-500 group-hover:rotate-12 drop-shadow-sm">📂</div>
                    <span className="text-[11px] font-black text-slate-700 truncate w-full text-center px-2 group-hover:text-indigo-700 uppercase tracking-tight">
                      {folder}
                    </span>
                  </div>

                  {/* Folder Actions */}
                  <div className="absolute -top-1 -right-1 opacity-0 group-hover:opacity-100 transition-all flex flex-col space-y-1 z-10">
                    <button
                      onClick={(e) => {
                        e.stopPropagation();
                        const newName = prompt('重命名文件夹:', folder);
                        if (newName && newName !== folder) {
                          onMove(fullPath, currentPath.length > 0 ? `${currentPath.join('/')}/${newName}` : newName, true);
                        }
                      }}
                      className="bg-white text-slate-400 w-6 h-6 rounded-lg flex items-center justify-center hover:text-indigo-600 shadow-sm border border-slate-100 hover:scale-110"
                    >
                      <span className="text-[10px]">✏️</span>
                    </button>
                    <button
                      onClick={(e) => {
                        e.stopPropagation();
                        if (confirm(`确定要删除文件夹 "${folder}" 及其所有内容吗？`)) {
                          onDelete(fullPath);
                        }
                      }}
                      className="bg-white text-slate-400 w-6 h-6 rounded-lg flex items-center justify-center hover:text-red-500 shadow-sm border border-slate-100 hover:scale-110"
                    >
                      <span className="text-[10px]">✕</span>
                    </button>
                  </div>
                </div>
              );
            })}

            {/* Files */}
            {filteredFiles.map(file => (
              <div
                key={file.id}
                draggable
                onDragStart={(e) => handleDragStart(e, file.filename, false)}
                className="group relative flex flex-col items-center p-4 rounded-[32px] hover:bg-white hover:shadow-2xl hover:shadow-indigo-100/40 cursor-default transition-all border border-transparent hover:scale-105"
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
                      const newName = prompt('重命名/移动文件:', currentName);
                      if (newName && newName !== currentName) {
                        const prefix = currentPath.length > 0 ? `${currentPath.join('/')}/` : '';
                        onMove(file.filename, prefix + newName, false);
                      }
                    }}
                    className="text-[10px] grayscale hover:grayscale-0 transition-all"
                  >
                    ✏️
                  </button>
                  <button
                    onClick={() => {
                      if (confirm(`确定要删除文件 "${file.filename}" 吗？`)) {
                        onDelete(file.filename);
                      }
                    }}
                    className="text-[10px] grayscale hover:grayscale-0 transition-all"
                  >
                    ✕
                  </button>
                </div>
              </div>
            ))}
          </div>
        ) : (
          <div className="flex flex-col space-y-2">
            {/* List View Header */}
            <div className="flex items-center px-8 py-3 bg-white/50 rounded-2xl text-[10px] font-black text-slate-400 uppercase tracking-widest border border-slate-100">
              <div className="flex-1">名称</div>
              <div className="w-32">大小</div>
              <div className="w-48 text-right">快捷操作</div>
            </div>

            {/* List Folders */}
            {filteredFolders.map(folder => {
              const fullPath = currentPath.length > 0 ? `${currentPath.join('/')}/${folder}` : folder;
              return (
                <div
                  key={folder}
                  draggable
                  onDragStart={(e) => handleDragStart(e, fullPath, true)}
                  onDragOver={(e) => e.preventDefault()}
                  onDrop={(e) => handleDrop(e, folder)}
                  onClick={() => onNavigate(folder)}
                  className="group flex items-center px-8 py-4 bg-white rounded-[24px] hover:shadow-xl hover:shadow-slate-200/40 transition-all border border-slate-50 cursor-pointer"
                >
                  <div className="flex-1 flex items-center">
                    <span className="text-3xl mr-4 group-hover:scale-110 transition-transform">📂</span>
                    <div>
                      <div className="text-xs font-black text-slate-700 uppercase tracking-tight">{folder}</div>
                      <div className="text-[9px] text-slate-300 font-bold uppercase mt-0.5">文件夹</div>
                    </div>
                  </div>
                  <div className="w-32 text-[10px] font-black text-slate-400">--</div>
                  <div className="w-48 flex justify-end space-x-2 opacity-0 group-hover:opacity-100 transition-opacity">
                    <button onClick={(e) => { e.stopPropagation(); onDelete(fullPath); }} className="w-8 h-8 rounded-lg hover:bg-red-50 text-red-400 flex items-center justify-center">✕</button>
                  </div>
                </div>
              );
            })}

            {/* List Files */}
            {filteredFiles.map(file => (
              <div
                key={file.id}
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
                </div>
              </div>
            ))}
          </div>
        )}

        {filteredFolders.length === 0 && filteredFiles.length === 0 && !isCreating && (
          <div className="h-full flex flex-col items-center justify-center py-40">
            <div className="text-9xl mb-10 grayscale opacity-10 animate-pulse">{searchQuery ? '🔎' : '☁️'}</div>
            <p className="text-slate-300 font-black text-2xl uppercase tracking-[0.2em]">{searchQuery ? `未找到 "${searchQuery}"` : '拖拽文件至此'}</p>
          </div>
        )}
      </div>
    </div>
  );
}
