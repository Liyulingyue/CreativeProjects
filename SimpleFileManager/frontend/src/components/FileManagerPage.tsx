import React, { useState, useEffect, useCallback, useRef } from 'react';
import {
  fetchBrowse, fetchTree, createFolder, deletePath, movePath, searchFiles,
  uploadFiles, downloadFile, copyPath,
  type BrowseResult, type FileNode, type TreeNode,
} from '../api';
import { FileGrid, FileList, FileCompactList, Toolbar, Breadcrumb, isPreviewable, formatSize } from './FileExplorer';
import { Sidebar } from './Sidebar';
import { FilePreview } from './FilePreview';
import ContextMenu from './ui/ContextMenu';
import { ConfirmDialog, PromptDialog } from './ui/Dialog';
import { useToast } from './ui/Toast';
import { Icon } from './ui/Icon';

type ViewMode = 'grid' | 'list' | 'compact';

const PAGE_SIZE = 200;

function LoadMoreSentinel({ hasMore, shown, total, onLoadMore }: { hasMore: boolean; shown: number; total: number; onLoadMore: () => void }) {
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!hasMore || !ref.current) return;
    const observer = new IntersectionObserver(
      entries => { if (entries[0].isIntersecting) onLoadMore(); },
      { rootMargin: '200px' }
    );
    observer.observe(ref.current);
    return () => observer.disconnect();
  }, [hasMore, onLoadMore]);

  if (!hasMore) return null;

  return (
    <div ref={ref} className="py-6 text-center text-sm text-slate-400">
      正在加载更多...（已显示 {shown} / {total}）
    </div>
  );
}

export function FileManagerPage() {
  const { toast } = useToast();

  const [browseResult, setBrowseResult] = useState<BrowseResult | null>(null);
  const [tree, setTree] = useState<TreeNode | null>(null);
  const [currentPath, setCurrentPath] = useState<string>('');
  const [selectedPaths, setSelectedPaths] = useState<Set<string>>(new Set());
  const [lastSelectedPath, setLastSelectedPath] = useState<string | null>(null);
  const [searchQuery, setSearchQuery] = useState('');
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [viewMode, setViewMode] = useState<ViewMode>('grid');
  const [sidebarOpen, setSidebarOpen] = useState(false);
  const [visibleCount, setVisibleCount] = useState(PAGE_SIZE);
  const [uploadProgress, setUploadProgress] = useState<{ loaded: number; total: number } | null>(null);
  const [clipboard, setClipboard] = useState<string[]>([]);
  const clipboardHasContent = clipboard.length > 0;
  const clipboardRef = useRef<string[]>([]);

  const [dragSource, setDragSource] = useState<FileNode | null>(null);
  const [dragOverFolder, setDragOverFolder] = useState<string | null>(null);

  const [isCreatingFolder, setIsCreatingFolder] = useState(false);
  const [newFolderName, setNewFolderName] = useState('');

  const [contextMenu, setContextMenu] = useState<{ x: number; y: number; node: FileNode } | null>(null);
  const [previewNode, setPreviewNode] = useState<FileNode | null>(null);

  const [confirmDialog, setConfirmDialog] = useState<{ open: boolean; title: string; message: string; onConfirm: () => void } | null>(null);
  const [promptDialog, setPromptDialog] = useState<{ open: boolean; title: string; message: string; defaultValue: string; onConfirm: (value: string) => void } | null>(null);

  const fileInputRef = useRef<HTMLInputElement>(null);

  const loadBrowse = useCallback(async (path?: string) => {
    setIsLoading(true);
    setError(null);
    setVisibleCount(PAGE_SIZE);
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
    loadBrowse();
    loadTree();
  }, [loadBrowse, loadTree]);

  const handleNavigate = (path: string) => {
    setSelectedPaths(new Set());
    setLastSelectedPath(null);
    loadBrowse(path);
    loadTree();
  };

  const handleSelect = (node: FileNode, e: React.MouseEvent) => {
    if (e.ctrlKey || e.metaKey) {
      setSelectedPaths(prev => {
        const next = new Set(prev);
        if (next.has(node.path)) {
          next.delete(node.path);
        } else {
          next.add(node.path);
        }
        return next;
      });
      setLastSelectedPath(node.path);
    } else if (e.shiftKey && lastSelectedPath && browseResult) {
      const items = browseResult.items;
      const startIdx = items.findIndex(i => i.path === lastSelectedPath);
      const endIdx = items.findIndex(i => i.path === node.path);
      if (startIdx !== -1 && endIdx !== -1) {
        const from = Math.min(startIdx, endIdx);
        const to = Math.max(startIdx, endIdx);
        const range = new Set(items.slice(from, to + 1).map(i => i.path));
        setSelectedPaths(range);
      }
    } else {
      if (node.is_dir) {
        handleNavigate(node.path);
      } else {
        setSelectedPaths(new Set([node.path]));
        setLastSelectedPath(node.path);
      }
    }
  };

  const handleDoubleClick = (node: FileNode) => {
    if (node.is_dir) {
      handleNavigate(node.path);
    } else if (isPreviewable(node)) {
      setPreviewNode(node);
    }
  };

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
    try {
      await movePath(dragSource.path, targetFolder + '/' + dragSource.name);
      setDragSource(null);
      setDragOverFolder(null);
      toast('已移动到 ' + targetFolder.split(/[/\\]/).pop(), 'success');
      loadBrowse(currentPath);
      loadTree();
    } catch (err) {
      toast('移动失败: ' + (err instanceof Error ? err.message : 'Unknown error'), 'error');
    }
  };

  const handleRefresh = () => {
    loadBrowse(currentPath);
    loadTree();
  };

  const handleSearch = async () => {
    if (!searchQuery.trim()) {
      loadBrowse(currentPath);
      return;
    }
    setIsLoading(true);
    setVisibleCount(PAGE_SIZE);
    try {
      const result = await searchFiles(searchQuery, currentPath);
      setBrowseResult({
        current_path: currentPath,
        parent_path: null,
        items: result.items,
        total_count: result.items.length,
        dirs_count: 0,
        files_count: result.items.length,
      });
      setSelectedPaths(new Set());
    } catch (err) {
      toast('搜索失败: ' + (err instanceof Error ? err.message : 'Unknown error'), 'error');
    } finally {
      setIsLoading(false);
    }
  };

  const handleContextMenu = (e: React.MouseEvent, node: FileNode) => {
    e.preventDefault();
    if (!selectedPaths.has(node.path)) {
      setSelectedPaths(new Set([node.path]));
      setLastSelectedPath(node.path);
    }
    setContextMenu({ x: e.clientX, y: e.clientY, node });
  };

  const handleDelete = (nodes?: FileNode | FileNode[]) => {
    const toDelete = nodes
      ? (Array.isArray(nodes) ? nodes : [nodes])
      : Array.from(selectedPaths).map(p => browseResult?.items.find(i => i.path === p)).filter(Boolean) as FileNode[];

    if (toDelete.length === 0) return;

    const names = toDelete.map(n => n.name).join(', ');
    const isMulti = toDelete.length > 1;
    const hasFolder = toDelete.some(n => n.is_dir);

    setConfirmDialog({
      open: true,
      title: `删除${isMulti ? `${toDelete.length} 项` : (hasFolder ? '文件夹' : '文件')}`,
      message: `确定要删除 "${names}" 吗？${hasFolder ? '文件夹内的所有内容将被删除。' : ''}此操作不可恢复。`,
      onConfirm: async () => {
        let succeeded = 0;
        let failed = 0;
        for (const node of toDelete) {
          try {
            await deletePath(node.path);
            succeeded++;
          } catch {
            failed++;
          }
        }
        if (succeeded > 0) {
          toast(`已删除 ${succeeded} 项${failed > 0 ? `，${failed} 项失败` : ''}`, failed > 0 ? 'error' : 'success');
        } else {
          toast('删除失败', 'error');
        }
        setSelectedPaths(new Set());
        setConfirmDialog(null);
        loadBrowse(currentPath);
        loadTree();
      },
    });
  };

  const handleMove = () => {
    if (selectedPaths.size === 0 || !tree) return;
    const firstPath = Array.from(selectedPaths)[0];
    setPromptDialog({
      open: true,
      title: selectedPaths.size > 1 ? `移动 ${selectedPaths.size} 项` : '移动',
      message: selectedPaths.size > 1
        ? `输入目标目录路径（${selectedPaths.size} 个文件将移动到此目录下）：`
        : '输入新的完整路径：',
      defaultValue: selectedPaths.size > 1 ? currentPath : firstPath,
      onConfirm: async (targetPath) => {
        let succeeded = 0;
        let failed = 0;
        for (const srcPath of selectedPaths) {
          const name = srcPath.split(/[/\\]/).pop() || '';
          const dest = selectedPaths.size > 1 ? targetPath + '/' + name : targetPath;
          try {
            await movePath(srcPath, dest);
            succeeded++;
          } catch {
            failed++;
          }
        }
        if (succeeded > 0) {
          toast(`已移动 ${succeeded} 项${failed > 0 ? `，${failed} 项失败` : ''}`, failed > 0 ? 'error' : 'success');
        } else {
          toast('移动失败', 'error');
        }
        setSelectedPaths(new Set());
        setPromptDialog(null);
        loadBrowse(currentPath);
        loadTree();
      },
    });
  };

  const handleCopy = () => {
    if (selectedPaths.size === 0) return;
    const paths = Array.from(selectedPaths);
    setClipboard(paths);
    clipboardRef.current = paths;
    toast(`已复制 ${paths.length} 项到剪贴板`, 'info');
  };

  const handlePaste = async () => {
    const paths = clipboardRef.current;
    if (paths.length === 0) return;
    let succeeded = 0;
    let failed = 0;
    for (const src of paths) {
      const name = src.split(/[/\\]/).pop() || '';
      const dest = currentPath + '/' + name;
      try {
        await copyPath(src, dest);
        succeeded++;
      } catch (err) {
        failed++;
        // handle name collision: append suffix
        const ext = name.includes('.') ? name.slice(name.lastIndexOf('.')) : '';
        const base = ext ? name.slice(0, -ext.length) : name;
        const altDest = `${currentPath}/${base}_copy${ext}`;
        try {
          await copyPath(src, altDest);
          succeeded++;
          failed--;
        } catch {
          // give up on this item
        }
      }
    }
    if (succeeded > 0) {
      toast(`已粘贴 ${succeeded} 项${failed > 0 ? `，${failed} 项失败` : ''}`, failed > 0 ? 'error' : 'success');
    } else {
      toast('粘贴失败', 'error');
    }
    loadBrowse(currentPath);
    loadTree();
  };

  const handleSelectAll = () => {
    if (!browseResult) return;
    setSelectedPaths(new Set(browseResult.items.map(i => i.path)));
  };

  const handleRenameSelected = () => {
    if (selectedPaths.size !== 1) return;
    const path = Array.from(selectedPaths)[0];
    const node = browseResult?.items.find(i => i.path === path);
    if (node) handleRename(node);
  };

  const handleCreateFolder = async () => {
    if (!newFolderName.trim()) return;
    try {
      await createFolder(currentPath, newFolderName.trim());
      toast('文件夹已创建', 'success');
      setIsCreatingFolder(false);
      setNewFolderName('');
      loadBrowse(currentPath);
      loadTree();
    } catch (err) {
      toast('创建失败: ' + (err instanceof Error ? err.message : 'Unknown error'), 'error');
    }
  };

  const handleRename = (node: FileNode) => {
    setPromptDialog({
      open: true,
      title: '重命名',
      message: '输入新名称：',
      defaultValue: node.name,
      onConfirm: async (newName) => {
        const fullPath = node.path;
        const sep = fullPath.includes('\\') ? '\\' : '/';
        const parentPath = fullPath.substring(0, fullPath.lastIndexOf(sep));
        const newPath = parentPath + sep + newName;
        try {
          await movePath(node.path, newPath);
          toast('已重命名', 'success');
          loadBrowse(currentPath);
          loadTree();
        } catch (err) {
          toast('重命名失败: ' + (err instanceof Error ? err.message : 'Unknown error'), 'error');
        }
        setPromptDialog(null);
      },
    });
  };

  const handleDownload = (node?: FileNode) => {
    if (node) {
      downloadFile(node.path).then(() => toast('开始下载 ' + node.name, 'info')).catch(() => toast('下载失败', 'error'));
    } else if (selectedPaths.size === 1) {
      const path = Array.from(selectedPaths)[0];
      const node = browseResult?.items.find(i => i.path === path);
      if (node && !node.is_dir) {
        downloadFile(node.path).then(() => toast('开始下载 ' + node.name, 'info')).catch(() => toast('下载失败', 'error'));
      }
    }
  };

  const handleUploadClick = () => {
    fileInputRef.current?.click();
  };

  const handleUpload = async (e: React.ChangeEvent<HTMLInputElement>) => {
    const files = e.target.files;
    if (!files || files.length === 0) return;
    const totalBytes = Array.from(files).reduce((sum, f) => sum + f.size, 0);
    setUploadProgress({ loaded: 0, total: totalBytes });
    try {
      const result = await uploadFiles(currentPath, files, (loaded, total) => setUploadProgress({ loaded, total }));
      toast(`已上传 ${result.count} 个文件`, 'success');
      loadBrowse(currentPath);
      loadTree();
    } catch (err) {
      toast('上传失败: ' + (err instanceof Error ? err.message : 'Unknown error'), 'error');
    } finally {
      setUploadProgress(null);
    }
    if (fileInputRef.current) {
      fileInputRef.current.value = '';
    }
  };

  // ---- keyboard shortcuts ----
  const handlersRef = useRef({ handleSelectAll, handleDelete, handleRenameSelected, handleRefresh, handleCopy, handlePaste });
  handlersRef.current = { handleSelectAll, handleDelete, handleRenameSelected, handleRefresh, handleCopy, handlePaste };

  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      const target = e.target as HTMLElement;
      if (target.tagName === 'INPUT' || target.tagName === 'TEXTAREA' || target.isContentEditable) return;
      // ignore when a modal or context menu is open
      if (confirmDialog || promptDialog || contextMenu || previewNode || isCreatingFolder) return;

      const sel = selectedPaths;
      const h = handlersRef.current;

      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'a') {
        e.preventDefault();
        h.handleSelectAll();
      } else if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'c') {
        e.preventDefault();
        h.handleCopy();
      } else if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'v') {
        e.preventDefault();
        h.handlePaste();
      } else if (e.key === 'Delete' || e.key === 'Backspace') {
        if (sel.size > 0) {
          e.preventDefault();
          h.handleDelete();
        }
      } else if (e.key === 'F2') {
        e.preventDefault();
        if (sel.size === 1) h.handleRenameSelected();
      } else if (e.key === 'F5') {
        e.preventDefault();
        h.handleRefresh();
      } else if (e.key === 'Escape') {
        if (sel.size > 0) setSelectedPaths(new Set());
      }
    };
    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
  }, [selectedPaths, confirmDialog, promptDialog, contextMenu, previewNode, isCreatingFolder]);

  const renderFileList = () => {
    if (!browseResult) return null;

    // 滚动加载：大目录只渲染前 PAGE_SIZE 项，滚到底加载更多
    const allItems = browseResult.items;
    const visibleItems = allItems.slice(0, visibleCount);
    const hasMore = visibleItems.length < allItems.length;

    const props = {
      items: visibleItems,
      selectedPaths,
      onSelect: handleSelect,
      onDoubleClick: handleDoubleClick,
      onContextMenu: handleContextMenu,
      onDragStart: handleDragStart,
      onDrop: handleDrop,
      onRename: handleRename,
      onDelete: (node: FileNode) => handleDelete(node),
      onDownload: (node: FileNode) => handleDownload(node),
      dragOverFolder,
      setDragOverFolder,
      onBack: () => browseResult.parent_path && handleNavigate(browseResult.parent_path),
      hasParent: !!browseResult.parent_path,
    };

    switch (viewMode) {
      case 'grid':
        return (
          <>
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
            <LoadMoreSentinel hasMore={hasMore} shown={visibleItems.length} total={allItems.length} onLoadMore={() => setVisibleCount(c => c + PAGE_SIZE)} />
          </>
        );
      case 'list':
        return (
          <>
            <FileList {...props} />
            <LoadMoreSentinel hasMore={hasMore} shown={visibleItems.length} total={allItems.length} onLoadMore={() => setVisibleCount(c => c + PAGE_SIZE)} />
          </>
        );
      case 'compact':
        return (
          <>
            <FileCompactList {...props} />
            <LoadMoreSentinel hasMore={hasMore} shown={visibleItems.length} total={allItems.length} onLoadMore={() => setVisibleCount(c => c + PAGE_SIZE)} />
          </>
        );
    }
  };

  return (
    <div className="flex flex-col h-full overflow-hidden">
      <input ref={fileInputRef} type="file" multiple className="hidden" onChange={handleUpload} />
      <Toolbar
        searchQuery={searchQuery}
        onSearchChange={setSearchQuery}
        onSearch={handleSearch}
        onRefresh={handleRefresh}
        onNewFolder={() => setIsCreatingFolder(true)}
        onDelete={() => handleDelete()}
        onMove={handleMove}
        onUpload={handleUploadClick}
        onDownload={() => handleDownload()}
        onCopy={handleCopy}
        onPaste={handlePaste}
        hasSelection={selectedPaths.size > 0}
        selectionCount={selectedPaths.size}
        clipboardHasContent={clipboardHasContent}
        viewMode={viewMode}
        onViewModeChange={setViewMode}
        onToggleSidebar={() => setSidebarOpen(v => !v)}
      />
      <div className="flex flex-1 overflow-hidden relative">
        {/* Mobile overlay */}
        {sidebarOpen && (
          <div className="fixed inset-0 z-30 bg-black/30 lg:hidden" onClick={() => setSidebarOpen(false)} />
        )}
        <Sidebar
          tree={tree}
          onNavigate={(p) => { handleNavigate(p); setSidebarOpen(false); }}
          currentPath={currentPath}
          mobileOpen={sidebarOpen}
        />
        <div className="flex-1 overflow-auto flex flex-col min-w-0">
          <div className="flex items-center gap-2 flex-shrink-0">
            <button
              className="lg:hidden p-2 ml-1 rounded-lg text-slate-500 hover:bg-slate-100"
              onClick={() => setSidebarOpen(true)}
            >
              <Icon name="folder" size={18} />
            </button>
            <div className="flex-1 min-w-0">
              <Breadcrumb path={currentPath} onNavigate={handleNavigate} />
            </div>
          </div>
          {uploadProgress && (
            <div className="px-4 py-2 bg-indigo-50 border-b border-indigo-100 flex-shrink-0">
              <div className="flex items-center justify-between text-xs text-indigo-700 mb-1">
                <span>正在上传 {uploadProgress.total > 0 ? formatSize(uploadProgress.loaded) + ' / ' + formatSize(uploadProgress.total) : ''}</span>
                <span>{uploadProgress.total > 0 ? Math.round((uploadProgress.loaded / uploadProgress.total) * 100) : 0}%</span>
              </div>
              <div className="h-1.5 bg-indigo-100 rounded-full overflow-hidden">
                <div
                  className="h-full bg-indigo-600 rounded-full transition-all duration-200"
                  style={{ width: `${uploadProgress.total > 0 ? (uploadProgress.loaded / uploadProgress.total) * 100 : 0}%` }}
                />
              </div>
            </div>
          )}
          {isLoading ? (
            <div className="flex items-center justify-center py-12">
              <div className="loading-spinner mr-3" />
              <span className="text-slate-500">加载中...</span>
            </div>
          ) : error ? (
            <div className="text-center py-12 text-red-500">{error}</div>
          ) : browseResult ? (
            <>
              <div className="px-4 py-1.5 text-xs text-slate-500 bg-white border-b border-slate-100 flex items-center justify-between flex-shrink-0">
                <span>{browseResult.total_count} 项 · {browseResult.dirs_count} 文件夹 · {browseResult.files_count} 文件</span>
                {selectedPaths.size > 0 && <span className="text-indigo-600 font-medium">已选 {selectedPaths.size} 项</span>}
              </div>
              <div className="flex-1 overflow-auto bg-slate-50/30">{renderFileList()}</div>
            </>
          ) : null}
        </div>
      </div>

      {contextMenu && (
        <ContextMenu
          x={contextMenu.x}
          y={contextMenu.y}
          items={[
            {
              label: contextMenu.node.is_dir ? '打开' : '预览',
              icon: contextMenu.node.is_dir ? 'folder' : 'eye',
              onClick: () => handleDoubleClick(contextMenu.node),
            },
            { label: '下载', icon: 'download', onClick: () => handleDownload(contextMenu.node) },
            { label: '复制', icon: 'copy', onClick: () => { setSelectedPaths(new Set([contextMenu.node.path])); setClipboard([contextMenu.node.path]); clipboardRef.current = [contextMenu.node.path]; toast('已复制到剪贴板', 'info'); } },
            { label: '重命名', icon: 'edit', onClick: () => handleRename(contextMenu.node) },
            { label: '删除', icon: 'trash', danger: true, onClick: () => handleDelete(contextMenu.node) },
          ]}
          onClose={() => setContextMenu(null)}
        />
      )}

      {previewNode && <FilePreview node={previewNode} onClose={() => setPreviewNode(null)} />}

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
