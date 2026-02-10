import React from 'react';

interface BreadcrumbsProps {
  currentPath: string[];
  onRoot: () => void;
  onBreadcrumbClick: (path: string[]) => void;
}

export default function Breadcrumbs({ currentPath, onRoot, onBreadcrumbClick }: BreadcrumbsProps) {
  return (
    <div className="flex items-center space-x-2 overflow-x-auto no-scrollbar">
      <button onClick={onRoot} className="w-10 h-10 flex items-center justify-center rounded-xl hover:bg-slate-100 transition-colors text-xl">🏠</button>
      <span className="text-slate-300 font-black">/</span>
      {currentPath.map((folder, idx) => (
        <div key={idx} className="flex items-center space-x-2 shrink-0">
          <button 
            onClick={() => onBreadcrumbClick(currentPath.slice(0, idx + 1))}
            className="px-3 py-1.5 rounded-xl hover:bg-slate-100 text-sm font-black text-slate-700 transition-colors uppercase tracking-tight"
          >
            {folder}
          </button>
          <span className="text-slate-300 font-black">/</span>
        </div>
      ))}
    </div>
  );
}
