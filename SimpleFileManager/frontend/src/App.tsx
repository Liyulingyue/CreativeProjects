import { useState, useEffect, useCallback } from 'react';
import { fetchBrowse, fetchTree, createFolder, deletePath, movePath, searchFiles, type BrowseResult, type FileNode, type TreeNode } from './api';
import { FileList } from './components/FileList';
import { Sidebar } from './components/Sidebar';
import { Toolbar } from './components/Toolbar';
import { Breadcrumb } from './components/Breadcrumb';
import { CreateFolderModal } from './components/CreateFolderModal';
import { MoveModal } from './components/MoveModal';

function App() {
  const [browseResult, setBrowseResult] = useState<BrowseResult | null>(null);
  const [tree, setTree] = useState<TreeNode | null>(null);
  const [currentPath, setCurrentPath] = useState<string>('');
  const [selectedPath, setSelectedPath] = useState<string | null>(null);
  const [searchQuery, setSearchQuery] = useState('');
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [showCreateFolder, setShowCreateFolder] = useState(false);
  const [showMove, setShowMove] = useState(false);

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

  const handleCreateFolder = async (name: string) => {
    try {
      await createFolder(currentPath, name);
      setShowCreateFolder(false);
      loadBrowse(currentPath);
    } catch (e) {
      alert('Failed to create folder: ' + (e instanceof Error ? e.message : 'Unknown error'));
    }
  };

  const handleDelete = async () => {
    if (!selectedPath) return;
    if (!confirm('Are you sure you want to delete this?')) return;
    try {
      await deletePath(selectedPath);
      setSelectedPath(null);
      loadBrowse(currentPath);
    } catch (e) {
      alert('Failed to delete: ' + (e instanceof Error ? e.message : 'Unknown error'));
    }
  };

  const handleMove = async (dest: string) => {
    if (!selectedPath) return;
    try {
      await movePath(selectedPath, dest);
      setShowMove(false);
      setSelectedPath(null);
      loadBrowse(currentPath);
      loadTree();
    } catch (e) {
      alert('Failed to move: ' + (e instanceof Error ? e.message : 'Unknown error'));
    }
  };

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

  return (
    <div className="app">
      <header className="header">
        <h1>SimpleFileManager</h1>
      </header>
      <Toolbar
        searchQuery={searchQuery}
        onSearchChange={setSearchQuery}
        onSearch={handleSearch}
        onRefresh={handleRefresh}
        onNewFolder={() => setShowCreateFolder(true)}
        onDelete={handleDelete}
        onMove={() => setShowMove(true)}
        hasSelection={!!selectedPath}
      />
      <div className="main-content">
        <Sidebar tree={tree} onNavigate={handleNavigate} currentPath={currentPath} />
        <div className="content">
          <Breadcrumb path={currentPath} onNavigate={handleNavigate} />
          {isLoading ? (
            <div className="loading">Loading...</div>
          ) : error ? (
            <div className="empty-state">
              <div className="empty-state-icon">!</div>
              <p>{error}</p>
            </div>
          ) : browseResult ? (
            <>
              <div className="stats-bar">
                <span>{browseResult.total_count} items</span>
                <span>{browseResult.dirs_count} folders</span>
                <span>{browseResult.files_count} files</span>
              </div>
              <FileList
                items={browseResult.items}
                selectedPath={selectedPath}
                onSelect={handleSelect}
                onDoubleClick={handleDoubleClick}
              />
            </>
          ) : null}
        </div>
      </div>

      {showCreateFolder && (
        <CreateFolderModal
          defaultName=""
          onConfirm={handleCreateFolder}
          onCancel={() => setShowCreateFolder(false)}
        />
      )}

      {showMove && selectedPath && (
        <MoveModal
          srcPath={selectedPath}
          tree={tree}
          onConfirm={handleMove}
          onCancel={() => setShowMove(false)}
        />
      )}
    </div>
  );
}

export default App;
