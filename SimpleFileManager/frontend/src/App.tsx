import React, { useState, useEffect, useCallback } from 'react';
import { fetchBrowse, fetchTree, createFolder, deletePath, movePath, searchFiles, type BrowseResult, type FileNode, type TreeNode } from './api';
import { RAGPanel } from './components/RAGPanel';
import { SimpleChat } from './components/SimpleChat';
import { FileGrid, FileList, FileCompactList, Toolbar, Breadcrumb } from './components/FileExplorer';
import { Sidebar } from './components/Sidebar';
import ContextMenu from './components/ui/ContextMenu';
import { ConfirmDialog, PromptDialog } from './components/ui/Dialog';

type Tab = 'files' | 'rag' | 'chat';
type ViewMode = 'grid' | 'list' | 'compact';

function App() {
  const [activeTab, setActiveTab] = useState<Tab>('files');
  const [browseResult, setBrowseResult] = useState<BrowseResult | null>(null);
  const [tree, setTree] = useState<TreeNode | null>(null);
  const [currentPath, setCurrentPath] = useState<string>('');
  const [selectedPath, setSelectedPath] = useState<string | null>(null);
  const [searchQuery, setSearchQuery] = useState('');
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [viewMode, setViewMode] = useState<ViewMode>('grid');

  // Drag and drop state
  const [dragSource, setDragSource] = useState<FileNode | null>(null);
  const [dragOverFolder, setDragOverFolder] = useState<string | null>(null);

  // Create folder state
  const [isCreatingFolder, setIsCreatingFolder] = useState(false);
  const [newFolderName, setNewFolderName] = useState('');

  // Context menu state
  const [contextMenu, setContextMenu] = useState<{ x: number; y: number; node: FileNode } | null>(null);

  // Dialog state
  const [confirmDialog, setConfirmDialog] = useState<{ open: boolean; title: string; message: string; onConfirm: () => void } | null>(null);
  const [promptDialog, setPromptDialog] = useState<{ open: boolean; title: string; message: string; defaultValue: string; onConfirm: (value: string) => void } | null>(null);

  const loadBrowse = useCallback(async (path?: string) => {
    setIsLoading(true);
    setError(null);
    try {
      const result = await fetchBrowse(path);
      setBrowseResult(result);
      setCurrentPath(result.current_path);
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Unknown error');
    } finally {
      setIsLoading(false);
    }
  }, []);

  const loadTree = useCallback(async () => {
    try {
      const result = await fetchTree();
      setTree(result);
    } catch (e) {
      console.error('Failed to load tree:', e);
    }
  }, []);

  useEffect(() => {
    if (activeTab === 'files') {
      loadBrowse();
      loadTree();
    }
  }, [activeTab, loadBrowse, loadTree]);

  const handleNavigate = (path: string) => {
    setSelectedPath(null);
    loadBrowse(path);
    loadTree();
  };

  const handleSelect = (node: FileNode) => {
    if (node.is_dir) {
      handleNavigate(node.path);
    } else {
      setSelectedPath(node.path);
    }
  };

  const handleDoubleClick = (node: FileNode) => {
    if (node.is_dir) {
      handleNavigate(node.path);
    }
  };

  // Drag and drop handlers
  const handleDragStart = (e: React.DragEvent, node: FileNode) => {
    setDragSource(node);
    e.dataTransfer.setData('sourcePath', node.path);
    e.dataTransfer.setData('isFolder', String(node.is_dir));
  };

  const handleDrop = async (e: React.DragEvent, targetFolder: string) => {
    e.preventDefault();
    if (!dragSource || dragSource.path === targetFolder) {
      setDragSource(null);
      return;
    }

    const fileName = dragSource.name;
    const dest = targetFolder.endsWith('/') ? targetFolder + fileName : targetFolder + '/' + fileName;

    try {
      await movePath(dragSource.path, dest);
      setDragSource(null);
      loadBrowse(currentPath);
      loadTree();
    } catch (err) {
      alert('移动失败: ' + (err instanceof Error ? err.message : 'Unknown error'));
    }
  };

  // Context menu handler
  const handleContextMenu = (e: React.MouseEvent, node: FileNode) => {
    e.preventDefault();
    setContextMenu({ x: e.clientX, y: e.clientY, node });
  };

  // Create folder handlers
  const handleCreateFolder = async () => {
    if (!newFolderName.trim()) {
      setIsCreatingFolder(false);
      setNewFolderName('');
      return;
    }
    try {
      await createFolder(currentPath, newFolderName);
      setIsCreatingFolder(false);
      setNewFolderName('');
      loadBrowse(currentPath);
      loadTree();
    } catch (err) {
      alert('创建失败: ' + (err instanceof Error ? err.message : 'Unknown error'));
    }
  };

  // Delete handler
  const handleDelete = async () => {
    if (!selectedPath) return;
    const item = browseResult?.items.find(i => i.path === selectedPath);
    setConfirmDialog({
      open: true,
      title: `删除${item?.is_dir ? '文件夹' : '文件'}`,
      message: `确定要删除 "${item?.name}" 吗？${item?.is_dir ? '文件夹内的所有内容将被删除。' : ''}`,
      onConfirm: async () => {
        try {
          await deletePath(selectedPath);
          setSelectedPath(null);
          loadBrowse(currentPath);
          loadTree();
        } catch (err) {
          alert('删除失败: ' + (err instanceof Error ? err.message : 'Unknown error'));
        }
        setConfirmDialog(null);
      }
    });
  };

  // Move handler
  const handleMove = () => {
    if (!selectedPath || !tree) return;
    setPromptDialog({
      open: true,
      title: '移动',
      message: '输入新的完整路径：',
      defaultValue: selectedPath,
      onConfirm: async (newPath) => {
        try {
          await movePath(selectedPath, newPath);
          setSelectedPath(null);
          loadBrowse(currentPath);
          loadTree();
        } catch (err) {
          alert('移动失败: ' + (err instanceof Error ? err.message : 'Unknown error'));
        }
        setPromptDialog(null);
      }
    });
  };

  // Search handler
  const handleSearch = async () => {
    if (!searchQuery.trim()) {
      loadBrowse(currentPath);
      return;
    }
    setIsLoading(true);
    setError(null);
    try {
      const result = await searchFiles(searchQuery, currentPath);
      setBrowseResult({
        current_path: currentPath,
        parent_path: browseResult?.parent_path || null,
        items: result.items,
        total_count: result.total,
        dirs_count: result.items.filter(i => i.is_dir).length,
        files_count: result.items.filter(i => !i.is_dir).length,
      });
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Unknown error');
    } finally {
      setIsLoading(false);
    }
  };

  const handleRefresh = () => {
    loadBrowse(currentPath);
    loadTree();
  };

  const renderFileList = () => {
    if (!browseResult) return null;

    const props = {
      items: browseResult.items,
      selectedPath,
      onSelect: handleSelect,
      onDoubleClick: handleDoubleClick,
      onContextMenu: handleContextMenu,
      onDragStart: handleDragStart,
      onDrop: handleDrop,
      dragOverFolder,
      setDragOverFolder,
      onBack: () => browseResult.parent_path && handleNavigate(browseResult.parent_path),
      hasParent: !!browseResult.parent_path,
    };

    switch (viewMode) {
      case 'grid':
        return (
          <FileGrid
            {...props}
            isCreatingFolder={isCreatingFolder}
            newFolderName={newFolderName}
            setNewFolderName={setNewFolderName}
            onCreateFolder={handleCreateFolder}
            onCancelCreateFolder={() => {
              setIsCreatingFolder(false);
              setNewFolderName('');
            }}
          />
        );
      case 'list':
        return <FileList {...props} />;
      case 'compact':
        return <FileCompactList {...props} />;
    }
  };

  return (
    <div className="app">
      <header className="flex items-center justify-between px-6 py-4 bg-white border-b border-slate-100">
        <h1 className="text-xl font-bold text-slate-900">📁 SimpleFileManager</h1>
        <div className="flex gap-2">
          <button
            className={`px-4 py-2 rounded-xl text-sm font-medium transition-colors ${activeTab === 'files' ? 'bg-indigo-600 text-white' : 'bg-slate-100 text-slate-600 hover:bg-slate-200'}`}
            onClick={() => setActiveTab('files')}
          >
            📂 文件管理
          </button>
          <button
            className={`px-4 py-2 rounded-xl text-sm font-medium transition-colors ${activeTab === 'rag' ? 'bg-indigo-600 text-white' : 'bg-slate-100 text-slate-600 hover:bg-slate-200'}`}
            onClick={() => setActiveTab('rag')}
          >
            📚 AI 问答
          </button>
          <button
            className={`px-4 py-2 rounded-xl text-sm font-medium transition-colors ${activeTab === 'chat' ? 'bg-indigo-600 text-white' : 'bg-slate-100 text-slate-600 hover:bg-slate-200'}`}
            onClick={() => setActiveTab('chat')}
          >
            💬 单纯对话
          </button>
        </div>
      </header>

      {activeTab === 'files' && (
        <>
          <Toolbar
            searchQuery={searchQuery}
            onSearchChange={setSearchQuery}
            onSearch={handleSearch}
            onRefresh={handleRefresh}
            onNewFolder={() => setIsCreatingFolder(true)}
            onDelete={handleDelete}
            onMove={handleMove}
            hasSelection={!!selectedPath}
            viewMode={viewMode}
            onViewModeChange={setViewMode}
          />
          <div className="flex" style={{ height: 'calc(100vh - 130px)' }}>
            <Sidebar tree={tree} onNavigate={handleNavigate} currentPath={currentPath} />
            <div className="flex-1 overflow-auto bg-slate-50/50">
              <Breadcrumb path={currentPath} onNavigate={handleNavigate} />
              {isLoading ? (
                <div className="flex items-center justify-center py-12">
                  <div className="loading-spinner mr-3" />
                  <span className="text-slate-500">加载中...</span>
                </div>
              ) : error ? (
                <div className="text-center py-12 text-red-500">{error}</div>
              ) : browseResult ? (
                <>
                  <div className="px-4 py-2 text-xs text-slate-500 bg-white border-b border-slate-100">
                    {browseResult.total_count} 项 | {browseResult.dirs_count} 文件夹 | {browseResult.files_count} 文件
                  </div>
                  {renderFileList()}
                </>
              ) : null}
            </div>
          </div>
        </>
      )}

      {activeTab === 'rag' && <RAGPanel />}
      {activeTab === 'chat' && <SimpleChat />}

      {/* Context Menu */}
      {contextMenu && (
        <ContextMenu
          x={contextMenu.x}
          y={contextMenu.y}
          items={[
            { label: '打开', icon: contextMenu.node.is_dir ? '📂' : '📄', onClick: () => handleDoubleClick(contextMenu.node) },
            { label: '重命名', icon: '✏️', onClick: () => {
              setPromptDialog({
                open: true,
                title: '重命名',
                message: '输入新名称：',
                defaultValue: contextMenu.node.name,
                onConfirm: async (newName) => {
                  const parentPath = contextMenu.node.path.substring(0, contextMenu.node.path.lastIndexOf('/'));
                  const newPath = parentPath + '/' + newName;
                  try {
                    await movePath(contextMenu.node.path, newPath);
                    loadBrowse(currentPath);
                    loadTree();
                  } catch (err) {
                    alert('重命名失败: ' + (err instanceof Error ? err.message : 'Unknown error'));
                  }
                  setPromptDialog(null);
                }
              });
            }},
            { label: '删除', icon: '🗑', danger: true, onClick: () => {
              setSelectedPath(contextMenu.node.path);
              handleDelete();
            }},
          ]}
          onClose={() => setContextMenu(null)}
        />
      )}

      {/* Confirm Dialog */}
      {confirmDialog && (
        <ConfirmDialog
          open={confirmDialog.open}
          title={confirmDialog.title}
          message={confirmDialog.message}
          onConfirm={confirmDialog.onConfirm}
          onCancel={() => setConfirmDialog(null)}
          danger
        />
      )}

      {/* Prompt Dialog */}
      {promptDialog && (
        <PromptDialog
          open={promptDialog.open}
          title={promptDialog.title}
          message={promptDialog.message}
          defaultValue={promptDialog.defaultValue}
          onConfirm={promptDialog.onConfirm}
          onCancel={() => setPromptDialog(null)}
        />
      )}
    </div>
  );
}

export default App;
