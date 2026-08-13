import { useState, useEffect } from 'react';

interface Snapshot {
  date: string;
  file_count: number;
  files: number;
  dirs: number;
}

interface FileChange {
  path: string;
  name: string;
  change_type: string;
  size: number;
  modified: string;
}

interface Suggestion {
  id: string;
  type: string;
  priority: string;
  message: string;
  source_path: string | null;
  target_path: string | null;
  reason: string;
}

interface CompareResult {
  date_from: string;
  date_to: string;
  added_files: FileChange[];
  added_dirs: FileChange[];
  deleted_files: FileChange[];
  deleted_dirs: FileChange[];
  suggestions: Suggestion[];
}

export function OrganizerPage() {
  const [snapshots, setSnapshots] = useState<Snapshot[]>([]);
  const [latestSnapshot, setLatestSnapshot] = useState<{ has_snapshot: boolean; date?: string; files?: number; dirs?: number } | null>(null);
  const [compareResult, setCompareResult] = useState<CompareResult | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [isTakingSnapshot, setIsTakingSnapshot] = useState(false);

  const loadSnapshots = async () => {
    setIsLoading(true);
    try {
      const [snapRes, latestRes] = await Promise.all([
        fetch('/api/organizer/snapshots'),
        fetch('/api/organizer/latest'),
      ]);

      if (snapRes.ok) {
        const data = await snapRes.json();
        setSnapshots(data.snapshots || []);
      }

      if (latestRes.ok) {
        const data = await latestRes.json();
        setLatestSnapshot(data);
      }
    } catch (e) {
      console.error('Failed to load:', e);
    } finally {
      setIsLoading(false);
    }
  };

  useEffect(() => {
    loadSnapshots();
  }, []);

  const handleTakeSnapshot = async () => {
    setIsTakingSnapshot(true);
    try {
      const res = await fetch('/api/organizer/snapshot', { method: 'POST' });
      if (res.ok) {
        await loadSnapshots();
      }
    } catch (e) {
      console.error('Failed to take snapshot:', e);
    } finally {
      setIsTakingSnapshot(false);
    }
  };

  const handleCompare = async (dateFrom: string, dateTo: string) => {
    setIsLoading(true);
    try {
      const res = await fetch(`/api/organizer/compare?date_from=${dateFrom}&date_to=${dateTo}`);
      if (res.ok) {
        const data = await res.json();
        setCompareResult(data);
      }
    } catch (e) {
      console.error('Failed to compare:', e);
    } finally {
      setIsLoading(false);
    }
  };

  const formatDate = (dateStr: string) => {
    const d = new Date(dateStr);
    return d.toLocaleDateString('zh-CN', { month: 'short', day: 'numeric', year: 'numeric' });
  };

  return (
    <div className="flex flex-col h-full bg-slate-50">
      {/* Header */}
      <div className="bg-white border-b border-slate-200 px-6 py-3 flex items-center justify-between">
        <div className="flex items-center gap-4">
          <div className="text-xl font-bold text-indigo-600">📊 文件整理</div>
          {latestSnapshot?.has_snapshot && (
            <div className="text-sm text-slate-500">
              最新快照: {formatDate(latestSnapshot.date!)} ({latestSnapshot.files} 文件, {latestSnapshot.dirs} 目录)
            </div>
          )}
        </div>
        <div className="flex gap-3">
          <button
            onClick={handleTakeSnapshot}
            disabled={isTakingSnapshot}
            className="px-4 py-2 rounded-lg bg-indigo-600 text-white text-sm font-medium hover:bg-indigo-700 disabled:opacity-50 transition-colors"
          >
            {isTakingSnapshot ? '拍摄中...' : '拍摄快照'}
          </button>
        </div>
      </div>

      {/* Content */}
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
        ) : (
          <div className="space-y-6">
            {/* Snapshot List */}
            <div className="bg-white rounded-2xl border border-slate-200 p-6">
              <div className="text-lg font-semibold text-slate-700 mb-4">历史快照</div>
              {snapshots.length === 0 ? (
                <div className="text-center py-8 text-slate-400">
                  <div className="text-4xl mb-2">📷</div>
                  <div>暂无快照</div>
                  <div className="text-sm mt-1">点击「拍摄快照」开始记录文件结构</div>
                </div>
              ) : (
                <div className="space-y-2">
                  {snapshots.map((snap, idx) => (
                    <div
                      key={snap.date}
                      className="flex items-center justify-between p-3 rounded-xl hover:bg-slate-50 transition-colors"
                    >
                      <div className="flex items-center gap-3">
                        <div className="w-10 h-10 rounded-lg bg-indigo-100 text-indigo-600 flex items-center justify-center font-bold">
                          {snap.file_count}
                        </div>
                        <div>
                          <div className="text-sm font-medium text-slate-700">{formatDate(snap.date)}</div>
                          <div className="text-xs text-slate-400">
                            {snap.files} 文件, {snap.dirs} 目录
                          </div>
                        </div>
                      </div>
                      {idx < snapshots.length - 1 && (
                        <button
                          onClick={() => handleCompare(snap.date, snapshots[idx + 1].date)}
                          className="px-3 py-1.5 rounded-lg bg-slate-100 text-slate-600 text-xs hover:bg-slate-200 transition-colors"
                        >
                          对比变化
                        </button>
                      )}
                    </div>
                  ))}
                </div>
              )}
            </div>

            {/* Compare Result */}
            {compareResult && (
              <div className="bg-white rounded-2xl border border-slate-200 p-6">
                <div className="text-lg font-semibold text-slate-700 mb-4">
                  变化分析: {formatDate(compareResult.date_to)} vs {formatDate(compareResult.date_from)}
                </div>

                <div className="grid grid-cols-4 gap-4 mb-6">
                  <div className="bg-green-50 rounded-xl p-4 text-center">
                    <div className="text-2xl font-bold text-green-600">{compareResult.added_files.length}</div>
                    <div className="text-xs text-slate-500">新增文件</div>
                  </div>
                  <div className="bg-blue-50 rounded-xl p-4 text-center">
                    <div className="text-2xl font-bold text-blue-600">{compareResult.added_dirs.length}</div>
                    <div className="text-xs text-slate-500">新增目录</div>
                  </div>
                  <div className="bg-red-50 rounded-xl p-4 text-center">
                    <div className="text-2xl font-bold text-red-600">{compareResult.deleted_files.length}</div>
                    <div className="text-xs text-slate-500">删除文件</div>
                  </div>
                  <div className="bg-orange-50 rounded-xl p-4 text-center">
                    <div className="text-2xl font-bold text-orange-600">{compareResult.deleted_dirs.length}</div>
                    <div className="text-xs text-slate-500">删除目录</div>
                  </div>
                </div>

                {/* Suggestions */}
                {compareResult.suggestions.length > 0 && (
                  <div>
                    <div className="text-sm font-medium text-slate-600 mb-3">💡 整理建议</div>
                    <div className="space-y-2">
                      {compareResult.suggestions.map(sug => (
                        <div key={sug.id} className="flex items-center gap-3 p-3 bg-slate-50 rounded-xl">
                          <div className={`w-8 h-8 rounded-lg flex items-center justify-center text-sm ${
                            sug.type === 'move' ? 'bg-blue-100 text-blue-600' : 'bg-orange-100 text-orange-600'
                          }`}>
                            {sug.type === 'move' ? '📁' : '📦'}
                          </div>
                          <div className="flex-1">
                            <div className="text-sm font-medium text-slate-700">{sug.message}</div>
                            {sug.source_path && (
                              <div className="text-xs text-slate-400 truncate">{sug.source_path}</div>
                            )}
                            <div className="text-xs text-slate-400">{sug.reason}</div>
                          </div>
                        </div>
                      ))}
                    </div>
                  </div>
                )}

                {/* New Files List */}
                {compareResult.added_files.length > 0 && (
                  <div className="mt-6">
                    <div className="text-sm font-medium text-slate-600 mb-3">📄 新增文件</div>
                    <div className="space-y-1 max-h-60 overflow-y-auto">
                      {compareResult.added_files.slice(0, 20).map((f, idx) => (
                        <div key={idx} className="flex items-center gap-2 p-2 rounded-lg hover:bg-slate-50">
                          <span className="text-sm text-slate-700 truncate">{f.name}</span>
                          <span className="text-xs text-slate-400">{f.path}</span>
                        </div>
                      ))}
                      {compareResult.added_files.length > 20 && (
                        <div className="text-xs text-slate-400 text-center py-2">
                          还有 {compareResult.added_files.length - 20} 个文件...
                        </div>
                      )}
                    </div>
                  </div>
                )}
              </div>
            )}
          </div>
        )}
      </div>
    </div>
  );
}
