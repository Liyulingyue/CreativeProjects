import { useState } from 'react';
import { type TreeNode } from '../api';

interface MoveModalProps {
  srcPath: string;
  tree: TreeNode | null;
  onConfirm: (dest: string) => void;
  onCancel: () => void;
}

function getFileName(path: string): string {
  return path.split(/[/\\]/).pop() || path;
}

function MoveTreeItem({
  node,
  level,
  selectedDest,
  onSelect,
}: {
  node: TreeNode;
  level: number;
  selectedDest: string | null;
  onSelect: (path: string) => void;
}) {
  const [expanded, setExpanded] = useState(true);
  const hasChildren = node.children && node.children.length > 0;
  const isSelected = selectedDest === node.path;

  return (
    <div>
      <div
        className={`tree-item ${isSelected ? 'active' : ''}`}
        style={{ paddingLeft: `${level * 1 + 0.5}rem` }}
        onClick={() => onSelect(node.path)}
      >
        {hasChildren ? (
          <span
            style={{ width: 16, textAlign: 'center', cursor: 'pointer' }}
            onClick={(e) => {
              e.stopPropagation();
              setExpanded(!expanded);
            }}
          >
            {expanded ? '▼' : '▶'}
          </span>
        ) : (
          <span style={{ width: 16 }} />
        )}
        <span>{node.is_dir ? '📁' : '📄'}</span>
        <span style={{ overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
          {node.name}
        </span>
      </div>
      {hasChildren && expanded && (
        <div className="tree-children">
          {node.children!.map((child) => (
            <MoveTreeItem
              key={child.path}
              node={child}
              level={level + 1}
              selectedDest={selectedDest}
              onSelect={onSelect}
            />
          ))}
        </div>
      )}
    </div>
  );
}

export function MoveModal({ srcPath, tree, onConfirm, onCancel }: MoveModalProps) {
  const fileName = getFileName(srcPath);
  const parentPath = srcPath.substring(0, srcPath.lastIndexOf('/') || srcPath.lastIndexOf('\\'));
  const [destPath, setDestPath] = useState<string>(parentPath);

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    const finalDest = destPath.endsWith('/') || destPath.endsWith('\\')
      ? destPath + fileName
      : destPath + '/' + fileName;
    onConfirm(finalDest);
  };

  return (
    <div className="modal-overlay" onClick={onCancel}>
      <div className="modal" onClick={(e) => e.stopPropagation()} style={{ width: 420 }}>
        <div className="modal-header">Move: {fileName}</div>
        <form onSubmit={handleSubmit}>
          <div className="modal-body">
            <p style={{ fontSize: '0.875rem', marginBottom: '1rem', color: 'var(--text-secondary)' }}>
              Select destination folder:
            </p>
            <div
              style={{
                maxHeight: 300,
                overflow: 'auto',
                border: '1px solid var(--border-color)',
                borderRadius: 6,
                padding: '0.5rem',
              }}
            >
              {tree ? (
                <MoveTreeItem node={tree} level={0} selectedDest={destPath} onSelect={setDestPath} />
              ) : (
                <div className="loading">Loading...</div>
              )}
            </div>
          </div>
          <div className="modal-footer">
            <button type="button" className="btn" onClick={onCancel}>
              Cancel
            </button>
            <button type="submit" className="btn btn-primary" disabled={!destPath}>
              Move Here
            </button>
          </div>
        </form>
      </div>
    </div>
  );
}
