import React from 'react';

interface ToolbarProps {
  searchQuery: string;
  setSearchQuery: (query: string) => void;
  viewMode: 'grid' | 'list' | 'compact';
  setViewMode: (mode: 'grid' | 'list' | 'compact') => void;
  onNewFolder: () => void;
  onUpload: () => void;
  children?: React.ReactNode;
}

export default function Toolbar({ 
  searchQuery, setSearchQuery, 
  viewMode, setViewMode, 
  onNewFolder, onUpload,
  children 
}: ToolbarProps) {
  return (
    <div className="h-20 border-b border-slate-100 flex items-center justify-between px-10 bg-white/80 backdrop-blur-md sticky top-0 z-20">
      {children}

      <div className="flex-1 max-w-md mx-8 group">
        <div className="relative flex items-center">
          <span className="absolute left-4 text-slate-300 group-focus-within:text-indigo-400 transition-colors">🔍</span>
          <input 
            type="text"
            placeholder="搜索当前目录..."
            value={searchQuery}
            onChange={(e) => setSearchQuery(e.target.value)}
            className="w-full pl-12 pr-4 py-2.5 bg-slate-50 border border-slate-100 rounded-2xl text-[11px] font-black uppercase tracking-tight focus:bg-white focus:border-indigo-300 focus:ring-4 focus:ring-indigo-50 outline-none transition-all"
          />
          {searchQuery && (
            <button 
              onClick={() => setSearchQuery('')}
              className="absolute right-3 w-6 h-6 flex items-center justify-center rounded-lg hover:bg-slate-200 text-slate-400 transition-colors"
            >
              ✕
            </button>
          )}
        </div>
      </div>

      <div className="flex items-center space-x-4">
        <div className="flex bg-slate-100 p-1 rounded-xl mr-2">
          <button 
            onClick={() => setViewMode('grid')}
            className={`px-3 py-1 rounded-lg text-[10px] font-black uppercase transition-all ${viewMode === 'grid' ? 'bg-white shadow-sm text-indigo-600' : 'text-slate-400'}`}
          >
            Grid
          </button>
          <button 
            onClick={() => setViewMode('list')}
            className={`px-3 py-1 rounded-lg text-[10px] font-black uppercase transition-all ${viewMode === 'list' ? 'bg-white shadow-sm text-indigo-600' : 'text-slate-400'}`}
          >
            List
          </button>
          <button 
            onClick={() => setViewMode('compact')}
            className={`px-3 py-1 rounded-lg text-[10px] font-black uppercase transition-all ${viewMode === 'compact' ? 'bg-white shadow-sm text-indigo-600' : 'text-slate-400'}`}
          >
            Compact
          </button>
        </div>
        <button 
          onClick={onNewFolder}
          className="flex items-center px-5 py-2.5 bg-indigo-50 text-indigo-700 rounded-2xl text-xs font-black uppercase tracking-widest hover:bg-indigo-100 transition-all active:scale-95"
        >
          <span className="mr-2 text-base">➕</span> 新文件夹
        </button>
        <button 
          onClick={onUpload}
          className="flex items-center px-6 py-3 bg-indigo-600 text-white rounded-2xl text-xs font-black uppercase tracking-widest hover:bg-indigo-700 shadow-xl shadow-indigo-200 transition-all active:scale-95 translate-y-[-2px] hover:translate-y-[-4px]"
        >
          <span className="mr-2 text-base">📤</span> 上传
        </button>
      </div>
    </div>
  );
}
