import { useState } from 'react';
import Breadcrumbs from './FileExplorer/Breadcrumbs';
import Toolbar from './FileExplorer/Toolbar';
import FileGrid from './FileExplorer/FileGrid';
import FileList from './FileExplorer/FileList';
import Dialog from './ui/Dialog';

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
  onBreadcrumbClick: (path: string[]) => void;
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
  onNavigate, onBreadcrumbClick, onBack, onRoot, onDownload,
  onCreateFolder, onUploadClick,
  onDelete, onMove
}: FileExplorerProps) {
  const [isCreating, setIsCreating] = useState(false);
  const [newFolderName, setNewFolderName] = useState('');
  const [viewMode, setViewMode] = useState<'grid' | 'list'>('grid');
  const [searchQuery, setSearchQuery] = useState('');
  const [dragOverFolder, setDragOverFolder] = useState<string | null>(null);

  // Custom Modal State
  const [modal, setModal] = useState<{
    show: boolean;
    type: 'confirm' | 'prompt';
    title: string;
    message: string;
    value: string;
    onOk: (val: string) => void;
    okText?: string;
  }>({
    show: false,
    type: 'confirm',
    title: '',
    message: '',
    value: '',
    onOk: () => {}
  });

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
    } else {
      setIsCreating(false);
    }
  };

  const cancelFolder = () => {
    setIsCreating(false);
    setNewFolderName('');
  };

  const openConfirm = (title: string, message: string, onOk: () => void) => {
    setModal({
      show: true,
      type: 'confirm',
      title,
      message,
      value: '',
      onOk: () => {
        onOk();
        setModal(m => ({ ...m, show: false }));
      },
      okText: '确认删除'
    });
  };

  const openPrompt = (title: string, message: string, initialValue: string, onOk: (v: string) => void) => {
    setModal({
      show: true,
      type: 'prompt',
      title,
      message,
      value: initialValue,
      onOk: (v) => {
        onOk(v);
        setModal(m => ({ ...m, show: false }));
      }
    });
  };

  return (
    <div className="flex-1 flex flex-col min-h-0 bg-white">
      <Toolbar 
        searchQuery={searchQuery}
        setSearchQuery={setSearchQuery}
        viewMode={viewMode}
        setViewMode={setViewMode}
        onNewFolder={() => setIsCreating(true)}
        onUpload={onUploadClick}
      >
        <Breadcrumbs 
          currentPath={currentPath}
          onRoot={onRoot}
          onBreadcrumbClick={onBreadcrumbClick}
        />
      </Toolbar>

      {/* Custom Dialog */}
      <Dialog 
        isOpen={modal.show} 
        onClose={() => setModal(m => ({ ...m, show: false }))}
        title={modal.title}
        footer={
          <>
            <button 
              onClick={() => setModal(m => ({ ...m, show: false }))}
              className="px-6 py-2 rounded-xl text-xs font-black text-slate-400 uppercase tracking-widest hover:bg-slate-100 transition-all"
            >
              取消
            </button>
            <button 
              onClick={() => modal.onOk(modal.value)}
              className={`px-6 py-2 rounded-xl text-xs font-black text-white uppercase tracking-widest transition-all ${modal.okText === '确认删除' ? 'bg-red-500 hover:bg-red-600 shadow-red-100' : 'bg-indigo-600 hover:bg-indigo-700 shadow-indigo-100'} shadow-lg`}
            >
              {modal.okText || '确认'}
            </button>
          </>
        }
      >
        <div className="space-y-4">
          <p className="text-sm text-slate-500 font-medium leading-relaxed">{modal.message}</p>
          {modal.type === 'prompt' && (
            <input 
              autoFocus
              value={modal.value}
              onChange={(e) => setModal(m => ({...m, value: e.target.value}))}
              className="w-full px-5 py-3 bg-slate-50 border border-slate-100 rounded-2xl text-xs font-black outline-none focus:bg-white focus:border-indigo-300 transition-all"
              onKeyDown={(e) => e.key === 'Enter' && modal.onOk(modal.value)}
            />
          )}
        </div>
      </Dialog>

      {/* Main Content Area */}
      <div className="flex-1 overflow-y-auto p-10 custom-scrollbar bg-slate-50/30">
        {viewMode === 'grid' ? (
          <FileGrid 
            currentPath={currentPath}
            filteredFolders={filteredFolders}
            filteredFiles={filteredFiles}
            isCreating={isCreating}
            newFolderName={newFolderName}
            dragOverFolder={dragOverFolder}
            setNewFolderName={setNewFolderName}
            onBack={onBack}
            onNavigate={onNavigate}
            onDownload={onDownload}
            onMove={onMove}
            onDelete={onDelete}
            submitFolder={submitFolder}
            cancelFolder={cancelFolder}
            handleDragStart={handleDragStart}
            handleDrop={handleDrop}
            setDragOverFolder={setDragOverFolder}
            openConfirm={openConfirm}
            openPrompt={openPrompt}
          />
        ) : (
          <FileList 
            currentPath={currentPath}
            filteredFolders={filteredFolders}
            filteredFiles={filteredFiles}
            isCreating={isCreating}
            newFolderName={newFolderName}
            dragOverFolder={dragOverFolder}
            setNewFolderName={setNewFolderName}
            onBack={onBack}
            onNavigate={onNavigate}
            onDownload={onDownload}
            onMove={onMove}
            onDelete={onDelete}
            submitFolder={submitFolder}
            cancelFolder={cancelFolder}
            handleDragStart={handleDragStart}
            handleDrop={handleDrop}
            setDragOverFolder={setDragOverFolder}
            openConfirm={openConfirm}
            openPrompt={openPrompt}
          />
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
