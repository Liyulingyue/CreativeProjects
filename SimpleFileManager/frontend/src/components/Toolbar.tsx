interface ToolbarProps {
  searchQuery: string;
  onSearchChange: (query: string) => void;
  onSearch: () => void;
  onRefresh: () => void;
  onNewFolder: () => void;
  onDelete: () => void;
  onMove: () => void;
  hasSelection: boolean;
}

export function Toolbar({
  searchQuery,
  onSearchChange,
  onSearch,
  onRefresh,
  onNewFolder,
  onDelete,
  onMove,
  hasSelection,
}: ToolbarProps) {
  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'Enter') {
      onSearch();
    }
  };

  return (
    <div className="toolbar">
      <div className="search-box">
        <span className="search-icon">🔍</span>
        <input
          type="text"
          className="search-input"
          placeholder="Search files..."
          value={searchQuery}
          onChange={(e) => onSearchChange(e.target.value)}
          onKeyDown={handleKeyDown}
        />
      </div>
      <button className="btn" onClick={onSearch} title="Search">
        Search
      </button>
      <button className="btn" onClick={onRefresh} title="Refresh">
        ↻ Refresh
      </button>
      <button className="btn btn-primary" onClick={onNewFolder} title="New Folder">
        + Folder
      </button>
      <button className="btn" onClick={onMove} disabled={!hasSelection} title="Move">
        ↗ Move
      </button>
      <button className="btn btn-danger" onClick={onDelete} disabled={!hasSelection} title="Delete">
        🗑 Delete
      </button>
    </div>
  );
}
