import { useEffect, useState } from 'react';
import { fetchFileContent, downloadFile, type FileNode } from '../api';
import { isImageFile, isTextFile, formatSize } from './FileExplorer/utils';

interface FilePreviewProps {
  node: FileNode;
  onClose: () => void;
}

export function FilePreview({ node, onClose }: FilePreviewProps) {
  const [content, setContent] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (isTextFile(node)) {
      setLoading(true);
      setError(null);
      fetchFileContent(node.path)
        .then(data => setContent(data.content))
        .catch(e => setError(e instanceof Error ? e.message : 'Failed to load'))
        .finally(() => setLoading(false));
    } else {
      setContent(null);
    }
  }, [node.path]);

  const isImg = isImageFile(node);
  const isPdf = node.extension.toLowerCase() === '.pdf';
  const isText = isTextFile(node);

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50" onClick={onClose}>
      <div
        className="relative bg-white rounded-2xl shadow-2xl max-w-4xl w-full mx-4 max-h-[90vh] flex flex-col overflow-hidden"
        onClick={e => e.stopPropagation()}
      >
        {/* Header */}
        <div className="flex items-center justify-between px-5 py-3 border-b border-slate-100 flex-shrink-0">
          <div className="flex items-center gap-3 min-w-0">
            <span className="text-lg flex-shrink-0">{isImg ? '🖼️' : isPdf ? '📕' : '📃'}</span>
            <div className="min-w-0">
              <div className="text-sm font-semibold text-slate-800 truncate">{node.name}</div>
              <div className="text-xs text-slate-400">{formatSize(node.size)}</div>
            </div>
          </div>
          <div className="flex items-center gap-2 flex-shrink-0">
            <button
              onClick={() => downloadFile(node.path)}
              className="px-3 py-1.5 rounded-lg bg-indigo-50 text-indigo-600 text-xs font-medium hover:bg-indigo-100 transition-colors"
            >
              ⬇ 下载
            </button>
            <button
              onClick={onClose}
              className="w-8 h-8 flex items-center justify-center rounded-lg text-slate-400 hover:text-slate-600 hover:bg-slate-100 transition-colors"
            >
              ✕
            </button>
          </div>
        </div>

        {/* Content */}
        <div className="flex-1 overflow-auto bg-slate-50">
          {isImg && (
            <div className="flex items-center justify-center p-8 min-h-full">
              <img
                src={`/api/fs/download?path=${encodeURIComponent(node.path)}`}
                alt={node.name}
                className="max-w-full max-h-[70vh] rounded-lg shadow-lg"
              />
            </div>
          )}

          {isPdf && (
            <iframe
              src={`/api/fs/download?path=${encodeURIComponent(node.path)}`}
              className="w-full h-[70vh] border-0"
              title={node.name}
            />
          )}

          {isText && (
            <>
              {loading ? (
                <div className="flex items-center justify-center py-12 text-slate-400">
                  <div className="loading-spinner mr-3" />
                  <span>加载中...</span>
                </div>
              ) : error ? (
                <div className="text-center py-12 text-red-500">{error}</div>
              ) : (
                <pre className="p-4 text-sm text-slate-700 font-mono whitespace-pre-wrap break-words">
                  {content}
                </pre>
              )}
            </>
          )}

          {!isImg && !isPdf && !isText && (
            <div className="flex flex-col items-center justify-center py-16 text-slate-400">
              <div className="text-5xl mb-4">📄</div>
              <div className="text-sm font-medium text-slate-600">{node.name}</div>
              <div className="text-xs mt-1">此文件类型不支持预览</div>
              <button
                onClick={() => downloadFile(node.path)}
                className="mt-4 px-4 py-2 rounded-lg bg-indigo-600 text-white text-sm font-medium hover:bg-indigo-700 transition-colors"
              >
                ⬇ 下载文件
              </button>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
