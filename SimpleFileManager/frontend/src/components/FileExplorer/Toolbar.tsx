import { Icon } from '../ui/Icon';

interface ToolbarProps {
  searchQuery: string;
  onSearchChange: (query: string) => void;
  onSearch: () => void;
  onRefresh: () => void;
  onNewFolder: () => void;
  onDelete: () => void;
  onMove: () => void;
  onUpload: () => void;
  onDownload: () => void;
  hasSelection: boolean;
  selectionCount: number;
  viewMode: 'grid' | 'list' | 'compact';
  onViewModeChange: (mode: 'grid' | 'list' | 'compact') => void;
  onToggleSidebar?: () => void;
}

const btnBase = 'p-2 rounded-lg transition-colors disabled:opacity-40 disabled:cursor-not-allowed';
const btnGhost = `${btnBase} text-slate-500 hover:bg-slate-100 hover:text-slate-700`;
const btnDanger = `${btnBase} text-slate-500 hover:bg-red-50 hover:text-red-600`;

export function Toolbar({
  searchQuery, onSearchChange, onSearch, onRefresh, onNewFolder,
  onDelete, onMove, onUpload, onDownload,
  hasSelection, selectionCount, viewMode, onViewModeChange,
}: ToolbarProps) {
  return (
    <div className="flex items-center gap-2 px-3 sm:px-4 py-2 bg-white border-b border-slate-200 flex-shrink-0">
      {/* Search */}
      <div className="relative flex-1 min-w-0">
        <span className="absolute left-3 top-1/2 -translate-y-1/2 text-slate-400">
          <Icon name="search" size={16} />
        </span>
        <input
          type="text"
          value={searchQuery}
          onChange={(e) => onSearchChange(e.target.value)}
          onKeyDown={(e) => e.key === 'Enter' && onSearch()}
          placeholder="搜索..."
          className="w-full pl-9 pr-3 py-1.5 rounded-lg border border-slate-200 bg-slate-50 text-sm placeholder:text-slate-400 focus:outline-none focus:ring-2 focus:ring-indigo-400/30 focus:border-indigo-400 transition-all"
        />
      </div>

      <div className="hidden sm:block" />

      {/* View Mode */}
      <div className="flex items-center bg-slate-100 rounded-lg p-0.5 flex-shrink-0">
        {([['grid', 'grid'], ['list', 'list'], ['compact', 'rows']] as const).map(([mode, icon]) => (
          <button
            key={mode}
            onClick={() => onViewModeChange(mode)}
            className={`p-1.5 rounded-md transition-all ${viewMode === mode ? 'bg-white shadow-sm text-indigo-600' : 'text-slate-400 hover:text-slate-600'}`}
            title={`${mode}视图`}
          >
            <Icon name={icon} size={16} />
          </button>
        ))}
      </div>

      <div className="w-px h-6 bg-slate-200 hidden sm:block" />

      {/* Actions */}
      <button onClick={onRefresh} className={btnGhost} title="刷新"><Icon name="refresh" size={18} /></button>
      <button onClick={onUpload} className={`${btnGhost} hidden sm:flex`} title="上传"><Icon name="upload" size={18} /></button>
      <button
        onClick={onNewFolder}
        className="flex items-center gap-1.5 px-2.5 sm:px-3 py-1.5 rounded-lg bg-indigo-600 text-white text-sm font-medium hover:bg-indigo-700 transition-colors"
      >
        <Icon name="plus" size={16} />
        <span className="hidden sm:inline">新建</span>
      </button>
      <button onClick={onDownload} disabled={!hasSelection || selectionCount > 1} className={`${btnGhost} hidden sm:flex`} title="下载"><Icon name="download" size={18} /></button>
      <button onClick={onMove} disabled={!hasSelection} className={`${btnGhost} hidden sm:flex`} title="移动"><Icon name="move" size={18} /></button>
      <button onClick={onDelete} disabled={!hasSelection} className={btnDanger} title="删除"><Icon name="trash" size={18} /></button>
    </div>
  );
}
