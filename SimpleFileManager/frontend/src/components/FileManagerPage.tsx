import React, { useState, useEffect, useCallback, useRef } from 'react';
import { collectDroppedFiles, relDirFromWebkitPath, type DroppedFile } from '../dropFiles';
import { useUploadQueue } from '../hooks/useUploadQueue';
import { addFavorite, fetchBrowse, fetchTree, createFolder, movePath, removeFavorite, searchFiles, uploadFileChunked, UploadConflictError, batchOperation, downloadFile, downloadZip, copyPath, type BrowseResult, type FileNode, type TreeNode } from '../api';
import { FileGrid, FileList, FileCompactList, Toolbar, Breadcrumb, isPreviewable, formatSize } from './FileExplorer';
import { Sidebar } from './Sidebar';
import { FilePreview } from './FilePreview';
import ContextMenu from './ui/ContextMenu';
import { ConfirmDialog, UploadConflictDialog, PromptDialog } from './ui/Dialog';
import { ShareDialog } from './ShareDialog';
import { VersionHistory } from './VersionHistory';
import { useToast } from './ui/Toast';
import { Icon } from './ui/Icon';
import { useCollapse } from '../hooks/useCollapse';

type ViewMode = 'grid' | 'list' | 'compact';

const PAGE_SIZE = 200;

// Internal moves tag the drag with `sourcePath`; an OS file drag only carries
// `Files`. Checking types rather than dragSource state also lets the dragenter
// handlers tell the two apart before any drop happens.
function isExternalFileDrag(e: React.DragEvent): boolean {
  const types = e.dataTransfer?.types;
  if (!types) return false;
  return Array.from(types).includes('Files') && !Array.from(types).includes('sourcePath');
}

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
  const { collapsed: treeCollapsed, toggle: toggleTree } = useCollapse('sfm.treeCollapsed');
  const [visibleCount, setVisibleCount] = useState(PAGE_SIZE);
  // The uploads, one line each, and the button that stops them.
  const queue = useUploadQueue();
  // These four do not change from one render to the next, and the
  // upload loop is built out of them -- taking the whole queue object
  // would rebuild the loop on every line of progress.
  const {
    update: updateQueue, setAll: setAllQueue,
    isCancelled: uploadsCancelled, signal: uploadSignal,
  } = queue;
  const [clipboard, setClipboard] = useState<string[]>([]);
  const clipboardHasContent = clipboard.length > 0;
  const clipboardRef = useRef<string[]>([]);

  const [dragSource, setDragSource] = useState<FileNode | null>(null);
  // True only while OS files hover the content area; drives the drop overlay.
  const [externalDrag, setExternalDrag] = useState(false);
  // dragleave fires for every child element, so track depth instead of
  // toggling on each event or the overlay flickers while moving the cursor.
  const dragDepthRef = useRef(0);
  const [dragOverFolder, setDragOverFolder] = useState<string | null>(null);

  const [isCreatingFolder, setIsCreatingFolder] = useState(false);
  const [newFolderName, setNewFolderName] = useState('');

  const [contextMenu, setContextMenu] = useState<{ x: number; y: number; node: FileNode } | null>(null);
  const [previewNode, setPreviewNode] = useState<FileNode | null>(null);
  // The files this person starred. Kept as a set of paths so the
  // right-click menu can say whether a file is already one.
  const [favoritePaths, setFavoritePaths] = useState<Set<string>>(new Set());

  const [confirmDialog, setConfirmDialog] = useState<{ open: boolean; title: string; message: string; onConfirm: () => void } | null>(null);
  // The file a share is being made for.
  const [shareTarget, setShareTarget] = useState<FileNode | null>(null);
  // The file whose history is being looked at.
  const [versionTarget, setVersionTarget] = useState<FileNode | null>(null);
  // An upload that stopped because a name was already taken. The whole
  // batch waits: the answer is about how to treat a name, and it
  // applies to the rest of the batch too.
  // The batch that was interrupted, so it can be finished.
  const uploadBatchRef = useRef<{
    list: DroppedFile[]; targetPath: string; done: number; failed: number;
  } | null>(null);
  const [uploadConflict, setUploadConflict] = useState<{
    /** The whole entry, so the folder it came from is kept. */
    entry: DroppedFile;
    targetPath: string;
    existing?: { name: string; size: number; modified?: string };
    /** Where in the batch we are, and what was already done. */
    index: number;
    done: number;
    failed: number;
  } | null>(null);
  const [promptDialog, setPromptDialog] = useState<{ open: boolean; title: string; message: string; defaultValue: string; onConfirm: (value: string) => void } | null>(null);

  const fileInputRef = useRef<HTMLInputElement>(null);
  // A second input that picks a folder rather than files. Browsers
  // spell this differently and some of them do not have it, so the
  // button hides itself when it is not available.
  const dirInputRef = useRef<HTMLInputElement>(null);

  const loadBrowse = useCallback(async (path?: string) => {
    setIsLoading(true);
    setError(null);
    setVisibleCount(PAGE_SIZE);
    try {
      const result = await fetchBrowse(path);
      setBrowseResult(result);
      setCurrentPath(result.current_path);
      setFavoritePaths(new Set(result.favorite_paths ?? []));
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

  // Tree leaves carry only {name, path, is_dir}; the preview needs an
  // extension, and files carry no size in the tree payload.
  const handleTreeOpenFile = (node: TreeNode) => {
    const name = node.name;
    const dot = name.lastIndexOf('.');
    const extension = dot > 0 ? name.slice(dot).toLowerCase() : '';
    const asFile: FileNode = {
      name,
      path: node.path,
      is_dir: false,
      extension,
      size: node.size ?? 0,
      modified: node.modified || '',
      created: '',
      mime_type: '',
    };
    if (isPreviewable(asFile)) {
      setPreviewNode(asFile);
    } else {
      // Not previewable (binary, archive, …): go to its folder instead of
      // asking the backend to browse a file path, which it rejects.
      const parent = node.path.slice(0, node.path.lastIndexOf('/'));
      handleNavigate(parent);
    }
  };

  const handleDragStart = (e: React.DragEvent, node: FileNode) => {
    setDragSource(node);
    e.dataTransfer.setData('sourcePath', node.path);
    e.dataTransfer.setData('isFolder', String(node.is_dir));
  };

  const handleDrop = async (e: React.DragEvent, targetFolder: string) => {
    e.preventDefault();
    e.stopPropagation();
    setDragOverFolder(null);

    // A folder card accepts two different drops: our own files move here, and
    // files dragged in from the OS are uploaded here.
    if (isExternalFileDrag(e)) {
      setExternalDrag(false);
      await uploadDrop(e.dataTransfer, targetFolder);
      return;
    }

    if (!dragSource || dragSource.path === targetFolder) {
      setDragSource(null);
      return;
    }
    try {
      await movePath(dragSource.path, targetFolder + '/' + dragSource.name);
      setDragSource(null);
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

  const isFavorite = (path: string) => favoritePaths.has(path);

  const toggleFavorite = async (node: FileNode) => {
    try {
      if (favoritePaths.has(node.path)) {
        await removeFavorite(node.path);
        setFavoritePaths(prev => {
          const next = new Set(prev);
          next.delete(node.path);
          return next;
        });
        toast('已取消收藏', 'info');
      } else {
        await addFavorite(node.path, node.name, node.is_dir);
        setFavoritePaths(prev => new Set(prev).add(node.path));
        toast('已收藏', 'success');
      }
    } catch (err) {
      toast(err instanceof Error ? err.message : '操作失败', 'error');
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
      message: `确定要删除 "${names}" 吗？${hasFolder ? '文件夹内的所有内容将被删除。' : ''}删除的文件会移到回收站，可以随时还原。`,
      onConfirm: async () => {
        try {
          const result = await batchOperation('delete', toDelete.map(n => n.path));
          if (result.count > 0) {
            toast(
              `已删除 ${result.count} 项${result.failed_count > 0 ? `，${result.failed_count} 项失败` : ''}`,
              result.failed_count > 0 ? 'error' : 'success',
            );
            if (result.failed_count > 0) {
              toast(`失败原因：${result.failed[0].error}`, 'error');
            }
          } else {
            toast('删除失败', 'error');
          }
        } catch (err) {
          toast('删除失败: ' + (err instanceof Error ? err.message : '未知错误'), 'error');
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
        try {
          // Moving several things into a directory keeps their names;
          // moving one thing means the target is the new path itself.
          if (selectedPaths.size > 1) {
            const result = await batchOperation('move', Array.from(selectedPaths), targetPath);
            if (result.count > 0) {
              toast(
                `已移动 ${result.count} 项${result.failed_count > 0 ? `，${result.failed_count} 项失败` : ''}`,
                result.failed_count > 0 ? 'error' : 'success',
              );
            } else {
              toast('移动失败', 'error');
            }
          } else {
            const srcPath = Array.from(selectedPaths)[0];
            await movePath(srcPath, targetPath);
            toast('已移动', 'success');
          }
        } catch (err) {
          toast('移动失败: ' + (err instanceof Error ? err.message : '未知错误'), 'error');
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
    // A browser cannot download a directory, so one is packed into a
    // zip first. The server builds it into a file and streams it.
    const grab = (n: FileNode) => {
      const job = n.is_dir ? downloadZip(n.path) : downloadFile(n.path);
      job
        .then(() => toast(n.is_dir ? `正在打包下载 ${n.name}` : '开始下载 ' + n.name, 'info'))
        .catch(() => toast('下载失败', 'error'));
    };
    if (node) {
      grab(node);
    } else if (selectedPaths.size === 1) {
      const path = Array.from(selectedPaths)[0];
      const node = browseResult?.items.find(i => i.path === path);
      if (node) grab(node);
    }
  };

  const handleUploadClick = () => {
    fileInputRef.current?.click();
  };

  const handleUploadFolderClick = () => {
    dirInputRef.current?.click();
  };

  // Shared by the file picker and OS drag-and-drop: both hand us a FileList,
  // so progress, toasts and refresh stay identical.
  // Upload one file, in pieces, and report what happened.
  const sendOneFile = useCallback(async (
    file: File,
    targetPath: string,
    overwrite: 'never' | 'replace' | 'rename',
    relDir = '',
    queueId = '',
  ) => {
    await uploadFileChunked(
      targetPath,
      file,
      (loaded) => {
        if (queueId) updateQueue(queueId, { loaded });
      },
      overwrite,
      relDir,
      uploadSignal(),
    );
  }, [updateQueue, uploadSignal]);

  // Upload a batch, one file at a time. A file whose name is taken
  // stops the batch and asks; the answer then applies to the rest of
  // it, so one question is asked rather than one per file.
  const runUpload = useCallback(async (
    list: DroppedFile[],
    targetPath: string,
    startIndex: number,
    policy: 'never' | 'replace' | 'rename',
    doneSoFar: number,
    failedSoFar: number,
  ) => {
    let done = doneSoFar;
    let failed = failedSoFar;

    for (let i = startIndex; i < list.length; i++) {
      const { file, relDir } = list[i];
      const queueId = list[i].id;
      if (uploadsCancelled()) {
        setAllQueue('cancelled');
        return;
      }
      updateQueue(queueId, { status: 'uploading', loaded: 0 });
      try {
        await sendOneFile(file, targetPath, policy, relDir, queueId);
        done++;
        updateQueue(queueId, { status: 'done', loaded: file.size });
      } catch (err) {
        if (err instanceof UploadConflictError) {
          // The batch is kept so it can be picked up again once the
          // user has said what to do.
          uploadBatchRef.current = { list, targetPath, done, failed };
          setUploadConflict({
            entry: list[i],
            targetPath,
            existing: err.conflict.existing,
            index: i,
            done,
            failed,
          });
          updateQueue(queueId, { status: 'waiting' });
          return;
        }
        if (uploadsCancelled()) {
          updateQueue(queueId, { status: 'cancelled' });
          return;
        }
        failed++;
        updateQueue(queueId, {
          status: 'failed',
          error: err instanceof Error ? err.message : '未知错误',
        });
        toast(`「${file.name}」上传失败：${err instanceof Error ? err.message : '未知错误'}`, 'error');
      }
    }

    uploadBatchRef.current = null;
    const where = targetPath ? `到「${targetPath.split(/[/\\]/).pop() || '根目录'}」` : '到根目录';
    if (done > 0) {
      toast(`已上传 ${done} 个文件${where}${failed ? `，${failed} 个失败` : ''}`, failed ? 'info' : 'success');
      loadBrowse(currentPath);
      loadTree();
    }
  }, [currentPath, sendOneFile, updateQueue, setAllQueue, uploadsCancelled]);

  const uploadFileList = useCallback(async (files: FileList | null | undefined, targetPath: string) => {
    if (!files || files.length === 0) return;
    const list: DroppedFile[] = Array.from(files).map(file => {
      const relDir = relDirFromWebkitPath((file as File & { webkitRelativePath?: string }).webkitRelativePath || '');
      return {
        file,
        relDir,
        depth: 0,
        id: `${relDir}/${file.name}:${file.size}:${file.lastModified}`,
      };
    });
    queue.start(list.map(f => ({ id: f.id, name: f.file.name, size: f.file.size })));
    await runUpload(list, targetPath, 0, 'never', 0, 0);
  }, [runUpload, queue]);

  // A drop may hold folders. The tree behind it is read first, so the
  // folder arrives with its shape instead of as one empty file.
  const uploadDrop = useCallback(async (dataTransfer: DataTransfer, targetPath: string) => {
    const list = await collectDroppedFiles(dataTransfer);
    if (!list.length) return;
    queue.start(list.map(f => ({ id: f.id, name: f.file.name, size: f.file.size })));
    await runUpload(list, targetPath, 0, 'never', 0, 0);
  }, [runUpload, queue]);

  const handleUpload = async (e: React.ChangeEvent<HTMLInputElement>) => {
    await uploadFileList(e.target.files, currentPath);
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
              thumbnailToken={browseResult?.thumbnail_token}
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
            <FileList {...props} thumbnailToken={browseResult?.thumbnail_token} />
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
      <input
        ref={dirInputRef}
        type="file"
        multiple
        className="hidden"
        // Non-standard, but it is how every browser that can pick a
        // folder spells picking a folder.
        {...{ webkitdirectory: '', directory: '' }}
        onChange={handleUpload}
      />
      <Toolbar
        searchQuery={searchQuery}
        onSearchChange={setSearchQuery}
        onSearch={handleSearch}
        onRefresh={handleRefresh}
        onNewFolder={() => setIsCreatingFolder(true)}
        onDelete={() => handleDelete()}
        onMove={handleMove}
        onUpload={handleUploadClick}
        onUploadFolder={handleUploadFolderClick}
        onDownload={() => handleDownload()}
        onCopy={handleCopy}
        onPaste={handlePaste}
        hasSelection={selectedPaths.size > 0}
        selectionCount={selectedPaths.size}
        clipboardHasContent={clipboardHasContent}
        viewMode={viewMode}
        onViewModeChange={setViewMode}
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
          onClose={() => setSidebarOpen(false)}
          collapsed={treeCollapsed}
          onToggleCollapse={toggleTree}
          onOpenFile={handleTreeOpenFile}
        />
        <div
          className="flex-1 overflow-auto flex flex-col min-w-0 relative"
          onDragEnter={(e) => {
            if (!isExternalFileDrag(e)) return;
            e.preventDefault();
            dragDepthRef.current += 1;
            setExternalDrag(true);
          }}
          onDragOver={(e) => {
            if (!isExternalFileDrag(e)) return;
            // Without preventDefault the browser navigates to the dropped file
            e.preventDefault();
            e.dataTransfer.dropEffect = 'copy';
          }}
          onDragLeave={(e) => {
            if (!isExternalFileDrag(e)) return;
            dragDepthRef.current = Math.max(0, dragDepthRef.current - 1);
            if (dragDepthRef.current === 0) setExternalDrag(false);
          }}
          onDrop={(e) => {
            // Folder cards handle their own drop and stop propagation; reaching
            // here means the cursor was over empty space, so upload here.
            if (!isExternalFileDrag(e)) return;
            e.preventDefault();
            dragDepthRef.current = 0;
            setExternalDrag(false);
            uploadDrop(e.dataTransfer, currentPath);
          }}
        >
          {externalDrag && (() => {
            // While hovering a folder card the drop lands there, not in the
            // current directory, so the overlay has to name the real target.
            const target = dragOverFolder ?? currentPath;
            const targetName = target.split(/[/\\]/).pop() || '根目录';
            return (
              <div className="absolute inset-0 z-20 bg-indigo-50/80 border-2 border-dashed border-indigo-400 pointer-events-none flex items-center justify-center">
                <div className="text-center">
                  <Icon name="upload" size={32} className="text-indigo-500 mx-auto mb-2" />
                  <div className="text-sm font-medium text-indigo-700">
                    松开上传到「{targetName}」
                  </div>
                  {dragOverFolder && (
                    <div className="text-xs text-indigo-500 mt-0.5">松开后进入该文件夹</div>
                  )}
                </div>
              </div>
            );
          })()}
          {/* The breadcrumb carries its own white background and border. The
              sidebar toggle used to sit outside it, so on a phone the page
              background showed through as a grey block to the left of the
              path — it read as a stray empty panel. One white bar for
              both. */}
          <div className="flex items-center gap-2 flex-shrink-0 bg-white border-b border-slate-100">
            <button
              className="lg:hidden p-2 ml-1 my-1 rounded-lg text-slate-500 hover:bg-slate-100 flex-shrink-0"
              onClick={() => setSidebarOpen(true)}
              title="展开目录树"
            >
              <Icon name="folder" size={18} />
            </button>
            <div className="flex-1 min-w-0">
              <Breadcrumb path={currentPath} onNavigate={handleNavigate} />
            </div>
          </div>
          {queue.items.length > 0 && (
            <div className="border-b border-slate-200 bg-slate-50 flex-shrink-0">
              <div className="flex items-center gap-3 px-4 py-2">
                <Icon name="upload" size={15} className="text-indigo-500 flex-shrink-0" />
                <div className="flex-1 min-w-0">
                  <div className="text-xs text-slate-700">
                    {queue.totals.done} / {queue.totals.total} 个文件
                    {queue.totals.failed > 0 && ` · ${queue.totals.failed} 个失败`}
                    {queue.totals.size > 0 && ` · ${formatSize(queue.totals.bytes)} / ${formatSize(queue.totals.size)}`}
                  </div>
                </div>
                {queue.isUploading && (
                  <button onClick={queue.cancel}
                    className="px-2.5 py-1 rounded-lg text-slate-600 text-xs font-medium hover:bg-slate-200 transition-colors flex-shrink-0">
                    停止
                  </button>
                )}
                <button onClick={() => queue.setOpen(o => !o)}
                  className="px-2.5 py-1 rounded-lg text-slate-600 text-xs font-medium hover:bg-slate-200 transition-colors flex-shrink-0">
                  {queue.open ? '收起' : '详情'}
                </button>
                {!queue.isUploading && (
                  <button onClick={queue.reset}
                    className="px-2.5 py-1 rounded-lg text-slate-600 text-xs font-medium hover:bg-slate-200 transition-colors flex-shrink-0">
                    清除
                  </button>
                )}
              </div>

              <div className="h-1 bg-slate-200 overflow-hidden">
                <div
                  className="h-full bg-indigo-500 transition-all duration-200"
                  style={{ width: `${queue.totals.size > 0 ? (queue.totals.bytes / queue.totals.size) * 100 : 0}%` }}
                />
              </div>

              {queue.open && (
                <div className="max-h-56 overflow-y-auto border-t border-slate-200 bg-white">
                  {queue.items.map(item => (
                    <div key={item.id} className="flex items-center gap-2 px-4 py-1.5 border-b border-slate-100 last:border-b-0">
                      <Icon
                        name={item.status === 'done' ? 'check' : item.status === 'failed' ? 'alert' : item.status === 'cancelled' ? 'close' : 'upload'}
                        size={13}
                        className={
                          item.status === 'done' ? 'text-emerald-500'
                            : item.status === 'failed' ? 'text-red-500'
                            : item.status === 'cancelled' ? 'text-slate-400'
                            : 'text-indigo-400'
                        }
                      />
                      <span className="text-xs text-slate-700 truncate flex-1" title={item.name}>
                        {item.name}
                      </span>
                      {item.error && <span className="text-[10px] text-red-500 truncate max-w-[10rem]">{item.error}</span>}
                      <span className="text-[10px] text-slate-400 flex-shrink-0">
                        {item.status === 'uploading'
                          ? `${formatSize(item.loaded)} / ${formatSize(item.size)}`
                          : formatSize(item.size)}
                      </span>
                    </div>
                  ))}
                </div>
              )}
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
              <div className="px-4 py-1.5 text-xs text-slate-500 bg-white border-b border-slate-100 flex items-center justify-between gap-2 flex-shrink-0">
                <span className="truncate">{browseResult.total_count} 项 · {browseResult.dirs_count} 文件夹 · {browseResult.files_count} 文件</span>
                <div className="flex items-center gap-3 flex-shrink-0">
                  {browseResult.disk?.ok && browseResult.disk.free !== undefined && (
                    <span className="hidden sm:inline" title="存储空间">
                      剩余 {formatSize(browseResult.disk.free)}
                    </span>
                  )}
                  {selectedPaths.size > 0 && <span className="text-indigo-600 font-medium">已选 {selectedPaths.size} 项</span>}
                </div>
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
            { label: '分享', icon: 'share', onClick: () => setShareTarget(contextMenu.node) },
            { label: '历史版本', icon: 'clock', onClick: () => setVersionTarget(contextMenu.node) },
            {
              label: isFavorite(contextMenu.node.path) ? '取消收藏' : '收藏',
              icon: 'star',
              onClick: () => toggleFavorite(contextMenu.node),
            },
            { label: '删除', icon: 'trash', danger: true, onClick: () => handleDelete(contextMenu.node) },
          ]}
          onClose={() => setContextMenu(null)}
        />
      )}

      {previewNode && <FilePreview node={previewNode} onClose={() => setPreviewNode(null)} />}

      {versionTarget && !versionTarget.is_dir && (
        <VersionHistory
          path={versionTarget.path}
          name={versionTarget.name}
          onClose={() => setVersionTarget(null)}
          onRestored={() => { loadBrowse(currentPath); toast('已还原到该版本', 'success'); }}
        />
      )}

      {shareTarget && (
        <ShareDialog
          path={shareTarget.path}
          name={shareTarget.name}
          isDir={shareTarget.is_dir}
          onClose={() => setShareTarget(null)}
          onCreated={() => toast('共享链接已创建，可在「共享链接」页管理', 'success')}
        />
      )}

      {uploadConflict && (
        <UploadConflictDialog
          open
          filename={uploadConflict.entry.file.name}
          existing={uploadConflict.existing}
          onCancel={() => {
            setUploadConflict(null);
            queue.setAll('cancelled');
            loadBrowse(currentPath);
          }}
          onRename={() => {
            const { entry, targetPath, index, done, failed } = uploadConflict;
            const batch = uploadBatchRef.current;
            setUploadConflict(null);
            void (async () => {
              // This file first, then the rest of the batch under the
              // same rule -- one answer, not one question per file.
              await runUpload([entry], targetPath, 0, 'rename', done, failed);
              if (batch) {
                await runUpload(batch.list, batch.targetPath, index + 1, 'rename', done + 1, failed);
              }
            })();
          }}
          onReplace={() => {
            const { entry, targetPath, index, done, failed } = uploadConflict;
            const batch = uploadBatchRef.current;
            setUploadConflict(null);
            void (async () => {
              await runUpload([entry], targetPath, 0, 'replace', done, failed);
              if (batch) {
                await runUpload(batch.list, batch.targetPath, index + 1, 'replace', done + 1, failed);
              }
            })();
          }}
        />
      )}

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
