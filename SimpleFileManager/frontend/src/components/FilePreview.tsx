import { useEffect, useState, useRef, useCallback } from 'react';
import { fetchFileContent, downloadFile, type FileNode, type FileContent } from '../api';
import { isImageFile, isTextFile, formatSize } from './FileExplorer/utils';
import { authFetch } from '../auth';
import { Icon } from './ui/Icon';

interface FilePreviewProps {
  node: FileNode;
  onClose: () => void;
}

const SEGMENT_CHARS = 50000;

function formatChars(n: number): string {
  if (n >= 10000) return `${(n / 10000).toFixed(1)} 万字`;
  return `${n} 字`;
}

export function FilePreview({ node, onClose }: FilePreviewProps) {
  const [content, setContent] = useState<string | null>(null);
  const [blobUrl, setBlobUrl] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [loadingMore, setLoadingMore] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [nextOffset, setNextOffset] = useState<number | null>(null);
  const [totalChars, setTotalChars] = useState(0);

  const scrollRef = useRef<HTMLDivElement>(null);
  const nextOffsetRef = useRef<number | null>(null);
  const loadingMoreRef = useRef(false);
  const pathRef = useRef(node.path);

  nextOffsetRef.current = nextOffset;

  // Appends a segment. `append` is false for the first load so a file switch
  // never leaves the previous file's text on screen.
  const loadSegment = useCallback(async (offset: number, append: boolean) => {
    const path = pathRef.current;
    const data: FileContent = await fetchFileContent(path, offset, SEGMENT_CHARS);
    // A late response for a file the user already navigated away from.
    if (pathRef.current !== path) return data;
    setContent(prev => (append && prev !== null ? prev + data.content : data.content));
    setNextOffset(data.next_offset);
    setTotalChars(data.total_chars);
    return data;
  }, []);

  useEffect(() => {
    pathRef.current = node.path;
    setBlobUrl(null);
    setContent(null);
    setError(null);
    setNextOffset(null);
    setTotalChars(0);
    loadingMoreRef.current = false;

    if (isTextFile(node)) {
      setLoading(true);
      loadSegment(0, false)
        .catch(e => setError(e instanceof Error ? e.message : 'Failed to load'))
        .finally(() => setLoading(false));
    } else if (isImageFile(node) || node.extension.toLowerCase() === '.pdf') {
      setLoading(true);
      authFetch(`/api/fs/download?path=${encodeURIComponent(node.path)}`)
        .then(res => res.blob())
        .then(blob => { setBlobUrl(URL.createObjectURL(blob)); })
        .catch(e => setError(e instanceof Error ? e.message : 'Failed to load'))
        .finally(() => setLoading(false));
    }

    return () => { if (blobUrl) URL.revokeObjectURL(blobUrl); };
  }, [node.path, node, loadSegment]);

  const loadMore = useCallback(async () => {
    const offset = nextOffsetRef.current;
    if (offset === null || loadingMoreRef.current) return;
    loadingMoreRef.current = true;
    setLoadingMore(true);
    try {
      await loadSegment(offset, true);
    } catch (e) {
      setError(e instanceof Error ? e.message : '加载更多失败');
    } finally {
      loadingMoreRef.current = false;
      setLoadingMore(false);
    }
  }, [loadSegment]);

  // Reading a novel should not require clicking "more" every screenful.
  const handleScroll = () => {
    const el = scrollRef.current;
    if (!el || nextOffsetRef.current === null || loadingMoreRef.current) return;
    const remaining = el.scrollHeight - el.scrollTop - el.clientHeight;
    if (remaining < 600) loadMore();
  };

  const isImg = isImageFile(node);
  const isPdf = node.extension.toLowerCase() === '.pdf';
  const isText = isTextFile(node);
  const loaded = content?.length ?? 0;
  const percent = totalChars > 0 ? Math.min(100, Math.round((loaded / totalChars) * 100)) : 100;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40 backdrop-blur-sm" onClick={onClose}>
      <div className="relative bg-white rounded-xl shadow-popover w-full max-w-4xl mx-2 sm:mx-4 mt-14 sm:mt-0 max-h-[85vh] sm:max-h-[90vh] flex flex-col overflow-hidden" onClick={e => e.stopPropagation()}>
        <div className="flex items-center justify-between px-5 py-3 border-b border-slate-100 flex-shrink-0">
          <div className="flex items-center gap-3 min-w-0">
            <div className="min-w-0">
              <div className="text-sm font-semibold text-slate-800 truncate">{node.name}</div>
              <div className="text-xs text-slate-400">
                {formatSize(node.size)}
                {isText && totalChars > loaded && (
                  <span className="ml-1">· 已读 {percent}%（{formatChars(loaded)} / {formatChars(totalChars)}）</span>
                )}
              </div>
            </div>
          </div>
          <div className="flex items-center gap-2 flex-shrink-0">
            <button onClick={() => downloadFile(node.path).catch(console.error)}
              className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg bg-slate-100 text-slate-700 text-xs font-medium hover:bg-slate-200 transition-colors">
              <Icon name="download" size={14} />下载
            </button>
            <button onClick={onClose} className="w-10 h-10 flex items-center justify-center rounded-lg text-slate-400 hover:text-slate-600 hover:bg-slate-100 transition-colors">
              <Icon name="x" size={18} />
            </button>
          </div>
        </div>

        <div ref={scrollRef} onScroll={handleScroll} className="flex-1 overflow-auto bg-slate-50">
          {loading && (
            <div className="flex items-center justify-center py-12 text-slate-400">
              <div className="loading-spinner mr-3" /><span>加载中...</span>
            </div>
          )}

          {error && <div className="text-center py-12 text-red-500">{error}</div>}

          {!loading && !error && isImg && blobUrl && (
            <div className="flex items-center justify-center p-8 min-h-full">
              <img src={blobUrl} alt={node.name} className="max-w-full max-h-[70vh] rounded-lg shadow-lg" />
            </div>
          )}

          {!loading && !error && isPdf && blobUrl && (
            <iframe src={blobUrl} className="w-full h-[70vh] border-0" title={node.name} />
          )}

          {!loading && !error && isText && (
            <>
              <pre className="p-4 text-sm text-slate-700 font-mono whitespace-pre-wrap break-words">{content}</pre>
              {nextOffset !== null && (
                <div className="px-4 pb-6 flex flex-col items-center gap-2">
                  <button
                    onClick={loadMore}
                    disabled={loadingMore}
                    className="flex items-center gap-1.5 px-4 py-2 rounded-lg bg-indigo-600 text-white text-sm font-medium hover:bg-indigo-700 disabled:opacity-50 transition-colors"
                  >
                    {loadingMore ? <div className="loading-spinner" /> : <Icon name="chevronDown" size={15} />}
                    {loadingMore ? '加载中...' : '加载更多'}
                  </button>
                  <div className="text-xs text-slate-400">
                    已显示 {percent}%（{formatChars(loaded)} / {formatChars(totalChars)}），向下滚动会自动续载
                  </div>
                </div>
              )}
              {nextOffset === null && loaded > 0 && (
                <div className="px-4 pb-6 text-center text-xs text-slate-400">
                  已读完，共 {formatChars(totalChars)}
                </div>
              )}
            </>
          )}

          {!loading && !error && !isImg && !isPdf && !isText && (
            <div className="flex flex-col items-center justify-center py-16 text-slate-400">
              <Icon name="file" size={48} className="text-slate-300 mb-3" />
              <div className="text-sm font-medium text-slate-600">{node.name}</div>
              <div className="text-xs mt-1">此文件类型不支持预览</div>
              <button onClick={() => downloadFile(node.path).catch(console.error)}
                className="mt-4 flex items-center gap-1.5 px-4 py-2 rounded-lg bg-indigo-600 text-white text-sm font-medium hover:bg-indigo-700 transition-colors">
                <Icon name="download" size={16} />下载文件
              </button>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}