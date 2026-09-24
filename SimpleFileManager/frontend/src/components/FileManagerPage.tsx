import React, { useState, useEffect, useCallback, useRef } from 'react';
import {
  fetchBrowse, fetchTree, createFolder, deletePath, movePath, searchFiles,
  uploadFiles, downloadFile,
  type BrowseResult, type FileNode, type TreeNode,
} from '../api';
import { FileGrid, FileList, FileCompactList, Toolbar, Breadcrumb, isPreviewable } from './FileExplorer';
import { Sidebar } from './Sidebar';
import { FilePreview } from './FilePreview';
import ContextMenu from './ui/ContextMenu';
import { ConfirmDialog, PromptDialog } from './ui/Dialog';
import { useToast } from './ui/Toast';

type ViewMode = 'grid' | 'list' | 'compact';

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
      downloadFile(node.path);
      toast('开始下载 ' + node.name, 'info');
    } else if (selectedPaths.size === 1) {
      const path = Array.from(selectedPaths)[0];
      const node = browseResult?.items.find(i => i.path === path);
      if (node && !node.is_dir) {
        downloadFile(node.path);
        toast('开始下载 ' + node.name, 'info');
      }
    }
  };

  const handleUploadClick = () => {
    fileInputRef.current?.click();
  };

  const handleUpload = async (e: React.ChangeEvent<HTMLInputElement>) => {
    const files = e.target.files;
    if (!files || files.length === 0) return;
    try {
      const result = await uploadFiles(currentPath, files);
      toast(`已上传 ${result.count} 个文件`, 'success');
      loadBrowse(currentPath);
      loadTree();
    } catch (err) {
      toast('上传失败: ' + (err instanceof Error ? err.message : 'Unknown error'), 'error');
    }
    if (fileInputRef.current) {
      fileInputRef.current.value = '';
    }
  };

  const renderFileList = () => {
    if (!browseResult) return null;

    const props = {
      items: browseResult.items,
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
        hasSelection={selectedPaths.size > 0}
        selectionCount={selectedPaths.size}
        viewMode={viewMode}
        onViewModeChange={setViewMode}
      />
      <div className="flex flex-1 overflow-hidden">
        <Sidebar tree={tree} onNavigate={handleNavigate} currentPath={currentPath} />
        <div className="flex-1 overflow-auto flex flex-col">
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
