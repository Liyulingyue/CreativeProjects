import { useState, useCallback, useRef, useEffect } from 'react';
import Breadcrumbs from './FileExplorer/Breadcrumbs';
import Toolbar from './FileExplorer/Toolbar';
import FileGrid from './FileExplorer/FileGrid';
import FileList from './FileExplorer/FileList';
import FileCompactList from './FileExplorer/FileCompactList';
import Dialog from './ui/Dialog';
import ContextMenu from './ui/ContextMenu';
import { useContextMenu } from '../hooks/useContextMenu';

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
  onMoveBatch: (moves: { oldPath: string, newPath: string, isFolder: boolean }[]) => void;
}

/**
 * FileExplorer 组件 - 一个高性能的文件管理基座。
 * 
 * 该组件旨在作为文件管理器的心脏，提供了现代化的文件交互功能（如：多选、右键菜单、拖拽移动等）。
 * 它具有极强的通用性和可扩展性，可以作为独立组件集成到个人 NAS 系统、私有云存储或 AI 数据集管理工具中。
 * 
 * @component
 * @param props 包含子文件夹列表、当前文件列表、路径导航回调及基础操作接口
 */
export default function FileExplorer({ 
  subFolders, currentFiles, currentPath, 
  onNavigate, onBreadcrumbClick, onBack, onRoot, onDownload,
  onCreateFolder, onUploadClick,
  onDelete, onMove, onMoveBatch
}: FileExplorerProps) {
  const [isCreating, setIsCreating] = useState(false);
  const [newFolderName, setNewFolderName] = useState('');
  const [viewMode, setViewMode] = useState<'grid' | 'list' | 'compact'>('list');
  const [searchQuery, setSearchQuery] = useState('');
  const [dragOverFolder, setDragOverFolder] = useState<string | null>(null);
  const [selectedFiles, setSelectedFiles] = useState<number[]>([]);
  const [selectedFolders, setSelectedFolders] = useState<string[]>([]);
  
  // Selection box state
  const [selection, setSelection] = useState<{
    active: boolean;
    startX: number;
    startY: number;
    currentX: number;
    currentY: number;
  }>({ active: false, startX: 0, startY: 0, currentX: 0, currentY: 0 });
  
  const containerRef = useRef<HTMLDivElement>(null);

  // Clear selection when path changes
  useEffect(() => {
    setSelectedFiles([]);
    setSelectedFolders([]);
    setSelection(prev => ({ ...prev, active: false }));
  }, [currentPath]);

  // Context Menu State
  const [contextMenu, setContextMenu] = useState<{
    x: number;
    y: number;
    show: boolean;
    items: any[];
  }>({ x: 0, y: 0, show: false, items: [] });

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

  const handleFileSelect = useCallback((fileId: number, ctrlKey: boolean) => {
    setSelectedFiles(prev => {
      if (ctrlKey) {
        // Ctrl+点击：多选/取消选择
        return prev.includes(fileId) 
          ? prev.filter(id => id !== fileId)
          : [...prev, fileId];
      } else {
        // 普通点击：单选
        return prev.includes(fileId) ? [] : [fileId];
      }
    });
  }, []);

  const handleMouseDown = (e: React.MouseEvent) => {
    // Only start selection on background or if not clicking an interactive element
    if (e.button !== 0) return; // Only left click
    
    const target = e.target as HTMLElement;
    if (target.closest('button') || target.closest('input') || target.closest('[draggable="true"]')) {
      return;
    }

    const rect = containerRef.current?.getBoundingClientRect();
    if (!rect) return;

    setSelection({
      active: true,
      startX: e.clientX - rect.left,
      startY: e.clientY - rect.top,
      currentX: e.clientX - rect.left,
      currentY: e.clientY - rect.top
    });
    
    // Clear selection if not holding ctrl
    if (!e.ctrlKey) {
      setSelectedFiles([]);
      setSelectedFolders([]);
    }
  };

  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        setSelectedFiles([]);
        setSelectedFolders([]);
        setSelection(s => ({ ...s, active: false }));
      }
    };
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, []);

  useEffect(() => {
    const handleMouseMove = (e: MouseEvent) => {
      if (!selection.active || !containerRef.current) return;
      
      const rect = containerRef.current.getBoundingClientRect();
      setSelection(prev => ({
        ...prev,
        currentX: e.clientX - rect.left,
        currentY: e.clientY - rect.top
      }));
    };

    const handleMouseUp = () => {
      if (!selection.active) return;

      // Calculate which files are in the selection box
      const box = {
        left: Math.min(selection.startX, selection.currentX),
        top: Math.min(selection.startY, selection.currentY),
        right: Math.max(selection.startX, selection.currentX),
        bottom: Math.max(selection.startY, selection.currentY)
      };

      if (containerRef.current) {
        const fileElements = containerRef.current.querySelectorAll('[data-file-id]');
        const folderElements = containerRef.current.querySelectorAll('[data-folder-name]');
        const newlySelectedFiles: number[] = [];
        const newlySelectedFolders: string[] = [];

        const containerRect = containerRef.current!.getBoundingClientRect();

        fileElements.forEach(el => {
          const elRect = el.getBoundingClientRect();
          const relativeRect = {
            left: elRect.left - containerRect.left,
            top: elRect.top - containerRect.top,
            right: elRect.right - containerRect.left,
            bottom: elRect.bottom - containerRect.top
          };
          if (!(relativeRect.left > box.right || relativeRect.right < box.left || 
                relativeRect.top > box.bottom || relativeRect.bottom < box.top)) {
            newlySelectedFiles.push(parseInt(el.getAttribute('data-file-id') || '0'));
          }
        });

        folderElements.forEach(el => {
          const elRect = el.getBoundingClientRect();
          const relativeRect = {
            left: elRect.left - containerRect.left,
            top: elRect.top - containerRect.top,
            right: elRect.right - containerRect.left,
            bottom: elRect.bottom - containerRect.top
          };
          if (!(relativeRect.left > box.right || relativeRect.right < box.left || 
                relativeRect.top > box.bottom || relativeRect.bottom < box.top)) {
            newlySelectedFolders.push(el.getAttribute('data-folder-name') || '');
          }
        });

        if (newlySelectedFiles.length > 0) {
          setSelectedFiles(prev => [...new Set([...prev, ...newlySelectedFiles])]);
        }
        if (newlySelectedFolders.length > 0) {
          setSelectedFolders(prev => [...new Set([...prev, ...newlySelectedFolders])]);
        }
      }

      setSelection(prev => ({ ...prev, active: false }));
    };

    if (selection.active) {
      window.addEventListener('mousemove', handleMouseMove);
      window.addEventListener('mouseup', handleMouseUp);
    }

    return () => {
      window.removeEventListener('mousemove', handleMouseMove);
      window.removeEventListener('mouseup', handleMouseUp);
    };
  }, [selection.active, selection.startX, selection.startY, selection.currentX, selection.currentY]);

  const getContextMenuItems = useContextMenu({
    currentPath,
    onNavigate,
    onMove,
    onDelete,
    onDownload,
    onCreateFolder,
    onUploadClick,
    setModal,
    onBatchDelete: (fileIds) => {
      fileIds.forEach(id => {
        const file = currentFiles.find(f => f.id === id);
        if (file) onDelete(file.filename);
      });
      setSelectedFiles([]);
    }
  });

  const handleDragStart = (e: React.DragEvent, path: string, isFolder: boolean) => {
    let filesToDrag = selectedFiles;
    let foldersToDrag = selectedFolders;

    if (isFolder) {
      const folderName = path.split('/').pop() || '';
      if (!selectedFolders.includes(folderName)) {
        foldersToDrag = [folderName];
        setSelectedFolders(foldersToDrag);
        // 如果点击的是文件夹且未选中，通常我们会清空文件选中
        setSelectedFiles([]);
        filesToDrag = [];
      }
    } else {
      const file = currentFiles.find(f => f.filename === path);
      if (file && !selectedFiles.includes(file.id)) {
        filesToDrag = [file.id];
        setSelectedFiles(filesToDrag);
        setSelectedFolders([]);
        foldersToDrag = [];
      }
    }

    e.dataTransfer.setData('sourceFiles', JSON.stringify(filesToDrag));
    e.dataTransfer.setData('sourceFolders', JSON.stringify(foldersToDrag));
    e.dataTransfer.setData('sourcePath', path);
    e.dataTransfer.setData('isFolder', String(isFolder));
    e.dataTransfer.effectAllowed = 'move';
  };

  const handleDrop = (e: React.DragEvent, targetFolder: string) => {
    e.preventDefault();
    const targetPath = currentPath.length > 0 
      ? `${currentPath.join('/')}/${targetFolder}`
      : targetFolder;

    const moves: { oldPath: string, newPath: string, isFolder: boolean }[] = [];

    // 批量移动文件夹
    const sourceFoldersStr = e.dataTransfer.getData('sourceFolders');
    if (sourceFoldersStr) {
      const folderNames: string[] = JSON.parse(sourceFoldersStr);
      folderNames.forEach(name => {
        const sourcePath = currentPath.length > 0 ? `${currentPath.join('/')}/${name}` : name;
        const newPath = `${targetPath}/${name}`;
        if (sourcePath !== newPath && name !== targetFolder) {
          moves.push({ oldPath: sourcePath, newPath, isFolder: true });
        }
      });
    }

    // 批量移动文件
    const sourceFilesStr = e.dataTransfer.getData('sourceFiles');
    if (sourceFilesStr) {
      const sourceFileIds: number[] = JSON.parse(sourceFilesStr);
      sourceFileIds.forEach(fileId => {
        const file = currentFiles.find(f => f.id === fileId);
        if (file) {
          const newPath = `${targetPath}/${file.filename.split('/').pop()}`;
          if (file.filename !== newPath) {
            moves.push({ oldPath: file.filename, newPath, isFolder: false });
          }
        }
      });
    }
    
    if (moves.length > 0) {
      onMoveBatch(moves);
    }
    
    setSelectedFiles([]);
    setSelectedFolders([]);
  };

  const handleDropToPath = (e: React.DragEvent, targetPathArray: string[]) => {
    e.preventDefault();
    const targetPath = targetPathArray.join('/');
    const moves: { oldPath: string, newPath: string, isFolder: boolean }[] = [];

    // 处理文件夹批量移动 (如果有的话)
    if (selectedFolders.length > 0) {
      selectedFolders.forEach(folderName => {
        const sourcePath = currentPath.length > 0 ? `${currentPath.join('/')}/${folderName}` : folderName;
        const newPath = targetPath ? `${targetPath}/${folderName}` : folderName;
        if (sourcePath !== newPath) {
          moves.push({ oldPath: sourcePath, newPath, isFolder: true });
        }
      });
    }

    // 也需要处理拖拽时可能没选中的那个单个对象
    const isFolderDrop = e.dataTransfer.getData('isFolder') === 'true';
    if (isFolderDrop && selectedFolders.length === 0) {
      const sourcePath = e.dataTransfer.getData('sourcePath');
      if (sourcePath) {
        const folderName = sourcePath.split('/').pop();
        const newPath = targetPath ? `${targetPath}/${folderName}` : (folderName || '');
        if (newPath && sourcePath !== newPath) {
          moves.push({ oldPath: sourcePath, newPath, isFolder: true });
        }
      }
    }

    // 处理文件批量移动
    const sourceFilesStr = e.dataTransfer.getData('sourceFiles');
    if (sourceFilesStr) {
      const sourceFileIds: number[] = JSON.parse(sourceFilesStr);
      sourceFileIds.forEach(fileId => {
        const file = currentFiles.find(f => f.id === fileId);
        if (file) {
          const fileName = file.filename.split('/').pop();
          const newPath = targetPath ? `${targetPath}/${fileName}` : (fileName || '');
          if (newPath && file.filename !== newPath) {
            moves.push({ oldPath: file.filename, newPath, isFolder: false });
          }
        }
      });
    }

    if (moves.length > 0) {
      onMoveBatch(moves);
    }

    setSelectedFiles([]);
    setSelectedFolders([]);
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

  const handleContextMenu = useCallback((e: React.MouseEvent, type: 'file' | 'folder' | 'background', data?: any) => {
    e.preventDefault();
    e.stopPropagation();

    let effectiveSelection = selectedFiles;
    
    // 如果右键点击的是未选中的文件，则更新有效选中为该文件
    if (type === 'file' && data && !selectedFiles.includes(data.id)) {
      effectiveSelection = [data.id];
      setSelectedFiles(effectiveSelection);
    } else if (type === 'folder' || type === 'background') {
      // 文件夹右键或背景右键暂时清空批量文件选中
      if (selectedFiles.length > 0) {
        effectiveSelection = [];
        setSelectedFiles([]);
      }
    }

    const items = getContextMenuItems(type, effectiveSelection, data);

    setContextMenu({
      x: e.clientX,
      y: e.clientY,
      show: true,
      items
    });
  }, [getContextMenuItems, selectedFiles]);

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
        <div className="flex flex-col">
          <Breadcrumbs 
            currentPath={currentPath}
            onRoot={onRoot}
            onBreadcrumbClick={onBreadcrumbClick}
            onDropToPath={handleDropToPath}
          />
          <span className="text-[10px] text-slate-400 font-medium ml-3 -mt-1 opacity-60">
            File Management Base / NAS-Ready Storage Core
          </span>
        </div>
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
      <div 
        ref={containerRef}
        className="flex-1 overflow-y-auto p-10 custom-scrollbar bg-slate-50/30 relative"
        onContextMenu={(e) => handleContextMenu(e, 'background')}
        onMouseDown={handleMouseDown}
      >
        {selection.active && (
          <div 
            className="absolute z-50 bg-indigo-500/20 border border-indigo-500/50 pointer-events-none"
            style={{
              left: Math.min(selection.startX, selection.currentX),
              top: Math.min(selection.startY, selection.currentY),
              width: Math.abs(selection.startX - selection.currentX),
              height: Math.abs(selection.startY - selection.currentY)
            }}
          />
        )}
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
            onContextMenu={handleContextMenu}
            selectedFiles={selectedFiles}
            selectedFolders={selectedFolders}
            onFileSelect={handleFileSelect}
          />
        ) : viewMode === 'list' ? (
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
            handleDropToPath={handleDropToPath}
            setDragOverFolder={setDragOverFolder}
            openConfirm={openConfirm}
            openPrompt={openPrompt}
            onContextMenu={handleContextMenu}
            selectedFiles={selectedFiles}
            selectedFolders={selectedFolders}
            onFileSelect={handleFileSelect}
          />
        ) : (
          <FileCompactList 
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
            handleDropToPath={handleDropToPath}
            setDragOverFolder={setDragOverFolder}
            openConfirm={openConfirm}
            openPrompt={openPrompt}
            onContextMenu={handleContextMenu}
            selectedFiles={selectedFiles}
            selectedFolders={selectedFolders}
            onFileSelect={handleFileSelect}
          />
        )}

        {filteredFolders.length === 0 && filteredFiles.length === 0 && !isCreating && (
          <div className="h-full flex flex-col items-center justify-center py-40">
            <div className="text-9xl mb-10 grayscale opacity-10 animate-pulse">{searchQuery ? '🔎' : '☁️'}</div>
            <p className="text-slate-300 font-black text-2xl uppercase tracking-[0.2em]">{searchQuery ? `未找到 "${searchQuery}"` : '拖拽文件至此'}</p>
          </div>
        )}
      </div>

      {/* Context Menu */}
      {contextMenu.show && (
        <ContextMenu
          x={contextMenu.x}
          y={contextMenu.y}
          items={contextMenu.items}
          onClose={() => setContextMenu(prev => ({ ...prev, show: false }))}
        />
      )}
    </div>
  );
}
