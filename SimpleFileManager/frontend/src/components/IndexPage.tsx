import { useState, useEffect } from 'react';

interface IndexedFile {
  id: string;
  file_path: string;
  content_preview: string;
}

interface IndexStats {
  indexed_count: number;
  vector_count: number;
}

export function IndexPage() {
  const [stats, setStats] = useState<IndexStats | null>(null);
  const [files, setFiles] = useState<IndexedFile[]>([]);
  const [isLoading, setIsLoading] = useState(true);
  const [isClearing, setIsClearing] = useState(false);

  const loadIndex = async () => {
    setIsLoading(true);
    try {
      const [statsRes, filesRes] = await Promise.all([
        fetch('/api/rag/status'),
        fetch('/api/rag/files'),
      ]);

      if (statsRes.ok) {
        const statsData = await statsRes.json();
        setStats(statsData);
      }

      if (filesRes.ok) {
        const filesData = await filesRes.json();
        setFiles(filesData.files || []);
      }
    } catch (e) {
      console.error('Failed to load index:', e);
    } finally {
      setIsLoading(false);
    }
  };

  useEffect(() => {
    loadIndex();
  }, []);

  const handleClearIndex = async () => {
    if (!confirm('确定要清空所有索引吗？此操作不可恢复。')) return;

    setIsClearing(true);
    try {
      const res = await fetch('/api/rag/clear', { method: 'DELETE' });
      if (res.ok) {
        setFiles([]);
        setStats({ indexed_count: 0, vector_count: 0 });
      }
    } catch (e) {
      console.error('Failed to clear index:', e);
    } finally {
      setIsClearing(false);
    }
  };

  const handleDeleteFile = async (filePath: string) => {
    if (!confirm(`确定要删除 "${filePath}" 的索引吗？`)) return;

    try {
      const res = await fetch(`/api/rag/files/${encodeURIComponent(filePath)}`, {
        method: 'DELETE',
      });
      if (res.ok) {
        setFiles(prev => prev.filter(f => f.file_path !== filePath));
        if (stats) {
          setStats({
            ...stats,
            indexed_count: Math.max(0, stats.indexed_count - 1),
            vector_count: Math.max(0, stats.vector_count - 1),
          });
        }
      }
    } catch (e) {
      console.error('Failed to delete file index:', e);
    }
  };

  return (
    <div className="flex flex-col h-full bg-slate-50">
      {/* Header */}
      <div className="bg-white border-b border-slate-200 px-6 py-3 flex items-center justify-between">
        <div className="flex gap-4">
          <div className="bg-indigo-50 rounded-lg px-4 py-2 text-center">
            <div className="text-xl font-bold text-indigo-600">{stats?.indexed_count ?? '-'}</div>
            <div className="text-xs text-slate-500">已索引</div>
          </div>
          <div className="bg-indigo-50 rounded-lg px-4 py-2 text-center">
            <div className="text-xl font-bold text-indigo-600">{stats?.vector_count ?? '-'}</div>
            <div className="text-xs text-slate-500">向量数</div>
          </div>
        </div>
        <div className="flex gap-3">
          <button
            onClick={loadIndex}
            className="px-4 py-2 rounded-lg bg-slate-100 text-slate-600 text-sm font-medium hover:bg-slate-200 transition-colors"
          >
            刷新
          </button>
          <button
            onClick={handleClearIndex}
            disabled={isClearing || (stats?.indexed_count ?? 0) === 0}
            className="px-4 py-2 rounded-lg bg-red-100 text-red-600 text-sm font-medium hover:bg-red-200 disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
          >
            {isClearing ? '清空中...' : '清空索引'}
          </button>
        </div>
      </div>

      {/* File List */}
      <div className="flex-1 overflow-y-auto p-6">
        {isLoading ? (
          <div className="flex items-center justify-center h-full text-slate-400">
            <div className="flex items-center gap-3">
              <div className="flex gap-1">
                <span className="w-3 h-3 bg-slate-400 rounded-full animate-bounce" style={{ animationDelay: '0ms' }} />
                <span className="w-3 h-3 bg-slate-400 rounded-full animate-bounce" style={{ animationDelay: '150ms' }} />
                <span className="w-3 h-3 bg-slate-400 rounded-full animate-bounce" style={{ animationDelay: '300ms' }} />
              </div>
              <span>加载中...</span>
            </div>
          </div>
        ) : files.length === 0 ? (
          <div className="text-center py-16 text-slate-400">
            <div className="text-5xl mb-4">📭</div>
            <div className="text-lg">暂无索引文件</div>
            <div className="text-sm mt-2">在文件管理中右键文件，选择「索引到向量库」</div>
          </div>
        ) : (
          <div className="space-y-3">
            <div className="text-sm text-slate-500 mb-4">已索引文件 ({files.length})</div>
            <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
              {files.map((file) => (
                <div
                  key={file.id}
                  className="bg-white rounded-xl border border-slate-200 p-4 hover:shadow-md transition-shadow"
                >
                  <div className="flex items-start justify-between gap-4">
                    <div className="flex items-center gap-2 mb-2">
                      <span className="text-xl">📄</span>
                      <div className="text-sm font-medium text-slate-700 truncate">
                        {file.file_path.split(/[/\\]/).pop()}
                      </div>
                    </div>
                    <button
                      onClick={() => handleDeleteFile(file.file_path)}
                      className="px-3 py-1.5 rounded-lg bg-red-50 text-red-600 text-xs font-medium hover:bg-red-100 transition-colors whitespace-nowrap"
                    >
                      删除
                    </button>
                  </div>
                  <div className="text-xs text-slate-400 truncate mb-2">
                    {file.file_path}
                  </div>
                  <div className="text-xs text-slate-500 bg-slate-50 rounded-lg p-2 line-clamp-2">
                    {file.content_preview}
                  </div>
                </div>
              ))}
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
