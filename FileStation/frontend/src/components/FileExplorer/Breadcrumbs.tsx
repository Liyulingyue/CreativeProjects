interface BreadcrumbsProps {
  currentPath: string[];
  onRoot: () => void;
  onBreadcrumbClick: (path: string[]) => void;
  onDropToPath: (e: React.DragEvent, path: string[]) => void;
}

export default function Breadcrumbs({ currentPath, onRoot, onBreadcrumbClick, onDropToPath }: BreadcrumbsProps) {
  const handleDragOver = (e: React.DragEvent) => {
    e.preventDefault();
    e.currentTarget.classList.add('bg-indigo-50', 'text-indigo-600', 'scale-110');
  };

  const handleDragLeave = (e: React.DragEvent) => {
    e.currentTarget.classList.remove('bg-indigo-50', 'text-indigo-600', 'scale-110');
  };

  const handleDrop = (e: React.DragEvent, path: string[]) => {
    e.preventDefault();
    e.currentTarget.classList.remove('bg-indigo-50', 'text-indigo-600', 'scale-110');
    onDropToPath(e, path);
  };

  return (
    <div className="flex items-center space-x-2 overflow-x-auto no-scrollbar">
      <button 
        onClick={onRoot} 
        onDragOver={handleDragOver}
        onDragLeave={handleDragLeave}
        onDrop={(e) => handleDrop(e, [])}
        className="w-10 h-10 flex items-center justify-center rounded-xl hover:bg-slate-100 transition-all text-xl"
      >
        🏠
      </button>
      <span className="text-slate-300 font-black">/</span>
      {currentPath.map((folder, idx) => (
        <div key={idx} className="flex items-center space-x-2 shrink-0">
          <button 
            onClick={() => onBreadcrumbClick(currentPath.slice(0, idx + 1))}
            onDragOver={handleDragOver}
            onDragLeave={handleDragLeave}
            onDrop={(e) => handleDrop(e, currentPath.slice(0, idx + 1))}
            className="px-3 py-1.5 rounded-xl hover:bg-slate-100 text-sm font-black text-slate-700 transition-all uppercase tracking-tight"
          >
            {folder}
          </button>
          <span className="text-slate-300 font-black">/</span>
        </div>
      ))}
    </div>
  );
}
