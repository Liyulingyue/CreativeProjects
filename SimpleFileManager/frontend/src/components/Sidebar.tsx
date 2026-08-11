import { useState } from 'react';
import type { TreeNode } from '../api';

interface SidebarProps {
  tree: TreeNode | null;
  onNavigate: (path: string) => void;
  currentPath: string;
}

interface TreeItemProps {
  node: TreeNode;
  level: number;
  currentPath: string;
  onNavigate: (path: string) => void;
}

function TreeItem({ node, level, currentPath, onNavigate }: TreeItemProps) {
  const [expanded, setExpanded] = useState(true);
  const isActive = currentPath === node.path;
  const hasChildren = node.children && node.children.length > 0;

  const handleClick = () => {
    if (node.is_dir) {
      if (hasChildren) {
        setExpanded(!expanded);
      }
      onNavigate(node.path);
    } else {
      onNavigate(node.path);
    }
  };

  return (
    <div className="w-60">
      <div
        className={`flex items-center gap-1 py-1 px-2 rounded cursor-pointer text-sm transition-colors ${
          isActive
            ? 'bg-indigo-100 text-indigo-700'
            : 'text-slate-600 hover:bg-slate-100'
        }`}
        style={{ paddingLeft: `${level * 12 + 8}px` }}
        onClick={handleClick}
      >
        {hasChildren ? (
          <span className="w-4 text-center text-xs text-slate-400 flex-shrink-0">
            {expanded ? '▼' : '▶'}
          </span>
        ) : (
          <span className="w-4 flex-shrink-0" />
        )}
        <span className="text-base flex-shrink-0">{node.is_dir ? '📁' : '📄'}</span>
        <span className="flex-1 min-w-0 truncate">{node.name}</span>
      </div>
      {hasChildren && expanded && (
        <div>
          {node.children!.map((child) => (
            <TreeItem
              key={child.path}
              node={child}
              level={level + 1}
              currentPath={currentPath}
              onNavigate={onNavigate}
            />
          ))}
        </div>
      )}
    </div>
  );
}

export function Sidebar({ tree, onNavigate, currentPath }: SidebarProps) {
  if (!tree) {
    return (
      <div className="w-60 bg-white border-r border-slate-200 flex flex-col">
        <div className="px-4 py-3 text-xs font-semibold text-slate-500 uppercase tracking-wider border-b border-slate-100">
          文件夹
        </div>
        <div className="flex-1 flex items-center justify-center text-slate-400 text-sm">
          加载中...
        </div>
      </div>
    );
  }

  return (
    <div className="w-60 bg-white border-r border-slate-200 flex flex-col overflow-hidden">
      <div className="px-4 py-3 text-xs font-semibold text-slate-500 uppercase tracking-wider border-b border-slate-100">
        文件夹
      </div>
      <div className="flex-1 overflow-y-auto py-2">
        <TreeItem node={tree} level={0} currentPath={currentPath} onNavigate={onNavigate} />
      </div>
    </div>
  );
}
