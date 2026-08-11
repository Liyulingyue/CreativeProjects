interface ToolbarProps {
  searchQuery: string;
  onSearchChange: (query: string) => void;
  onSearch: () => void;
  onRefresh: () => void;
  onNewFolder: () => void;
  onDelete: () => void;
  onMove: () => void;
  hasSelection: boolean;
  viewMode: 'grid' | 'list' | 'compact';
  onViewModeChange: (mode: 'grid' | 'list' | 'compact') => void;
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
  viewMode,
  onViewModeChange,
}: ToolbarProps) {
  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'Enter') {
      onSearch();
    }
  };

  return (
    <div className="flex items-center gap-3 px-4 py-2 bg-white border-b border-slate-100">
      {/* Search Box - fills remaining space */}
      <div className="relative flex-1">
        <span className="absolute left-3 top-1/2 -translate-y-1/2 text-slate-400">🔍</span>
        <input
          type="text"
          value={searchQuery}
          onChange={(e) => onSearchChange(e.target.value)}
          onKeyDown={handleKeyDown}
          placeholder="搜索文件..."
          className="w-full pl-10 pr-4 py-1.5 rounded-lg border border-slate-200 bg-slate-50 text-sm focus:outline-none focus:ring-2 focus:ring-indigo-500 focus:border-transparent transition-all"
        />
      </div>

      {/* View Mode Toggle */}
      <div className="flex items-center bg-slate-100 rounded-lg p-0.5">
        <button
          onClick={() => onViewModeChange('grid')}
          className={`p-1.5 rounded-md transition-all text-xs ${
            viewMode === 'grid'
              ? 'bg-white shadow-sm text-indigo-600'
              : 'text-slate-500 hover:text-slate-700'
          }`}
          title="网格视图"
        >
          ▦
        </button>
        <button
          onClick={() => onViewModeChange('list')}
          className={`p-1.5 rounded-md transition-all text-xs ${
            viewMode === 'list'
              ? 'bg-white shadow-sm text-indigo-600'
              : 'text-slate-500 hover:text-slate-700'
          }`}
          title="列表视图"
        >
          ☰
        </button>
        <button
          onClick={() => onViewModeChange('compact')}
          className={`p-1.5 rounded-md transition-all text-xs ${
            viewMode === 'compact'
              ? 'bg-white shadow-sm text-indigo-600'
              : 'text-slate-500 hover:text-slate-700'
          }`}
          title="紧凑视图"
        >
          ≡
        </button>
      </div>

      {/* Actions */}
      <div className="flex items-center gap-1">
        <button
          onClick={onRefresh}
          className="p-1.5 rounded-lg border border-slate-200 text-slate-500 hover:bg-slate-50 transition-colors text-sm"
          title="刷新"
        >
          ↻
        </button>
        <button
          onClick={onNewFolder}
          className="px-3 py-1.5 rounded-lg bg-indigo-600 text-white text-sm font-medium hover:bg-indigo-700 transition-colors flex items-center gap-1"
        >
          <span>+</span>
          <span>新建</span>
        </button>
        <button
          onClick={onMove}
          disabled={!hasSelection}
          className="px-2 py-1.5 rounded-lg border border-slate-200 text-slate-500 text-sm hover:bg-slate-50 transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
          title="移动"
        >
          ↗
        </button>
        <button
          onClick={onDelete}
          disabled={!hasSelection}
          className="px-2 py-1.5 rounded-lg border border-red-200 text-red-500 text-sm hover:bg-red-50 transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
          title="删除"
        >
          🗑
        </button>
      </div>
    </div>
  );
}
