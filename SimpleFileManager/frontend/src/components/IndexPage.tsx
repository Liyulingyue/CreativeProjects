import { useState, useEffect, useCallback } from 'react';
import { fetchAutoIndexStatus, runAutoIndexNow, updateSettings } from '../api';
import { useToast } from './ui/Toast';
import { ConfirmDialog } from './ui/Dialog';
import { Icon } from './ui/Icon';
import type { AutoIndexStatus } from '../types';

interface IndexedFile { id: string; file_path: string; content_preview: string; }
interface IndexStats { indexed_count: number; vector_count: number; }

export function IndexPage() {
  const { toast } = useToast();
  const [stats, setStats] = useState<IndexStats | null>(null);
  const [files, setFiles] = useState<IndexedFile[]>([]);
  const [isLoading, setIsLoading] = useState(true);
  const [isClearing, setIsClearing] = useState(false);
  const [autoStatus, setAutoStatus] = useState<AutoIndexStatus | null>(null);
  const [isToggling, setIsToggling] = useState(false);
  const [isScanning, setIsScanning] = useState(false);
  const [confirmClear, setConfirmClear] = useState(false);
  const [confirmDeleteFile, setConfirmDeleteFile] = useState<string | null>(null);

  const loadAutoStatus = useCallback(async () => {
    try { setAutoStatus(await fetchAutoIndexStatus()); } catch (e) { console.error('Failed:', e); }
  }, []);

  const loadIndex = async () => {
    setIsLoading(true);
    try {
      const [statsRes, filesRes] = await Promise.all([fetch('/api/rag/status'), fetch('/api/rag/files')]);
      if (statsRes.ok) setStats(await statsRes.json());
      if (filesRes.ok) setFiles((await filesRes.json()).files || []);
    } catch (e) { console.error('Failed:', e); } finally { setIsLoading(false); }
  };

  useEffect(() => {
    loadIndex(); loadAutoStatus();
    const timer = setInterval(loadAutoStatus, 10000);
    return () => clearInterval(timer);
  }, [loadAutoStatus]);

  const handleToggleAuto = async () => {
    if (!autoStatus) return;
    setIsToggling(true);
    try { await updateSettings({ auto_index_enabled: !autoStatus.enabled }); await loadAutoStatus(); toast(autoStatus.enabled ? '已关闭自动索引' : '已开启自动索引', 'success'); }
    catch { toast('切换失败', 'error'); } finally { setIsToggling(false); }
  };

  const handleScanNow = async () => {
    setIsScanning(true);
    try { await runAutoIndexNow(); await Promise.all([loadIndex(), loadAutoStatus()]); toast('扫描完成', 'success'); }
    catch { toast('扫描失败', 'error'); } finally { setIsScanning(false); }
  };

  const handleClearIndex = async () => {
    setIsClearing(true);
    try { await fetch('/api/rag/clear', { method: 'DELETE' }); setFiles([]); setStats({ indexed_count: 0, vector_count: 0 }); toast('索引已清空', 'success'); }
    catch { toast('清空失败', 'error'); } finally { setIsClearing(false); setConfirmClear(false); }
  };

  const handleDeleteFile = async (filePath: string) => {
    try {
      await fetch(`/api/rag/files/${encodeURIComponent(filePath)}`, { method: 'DELETE' });
      setFiles(prev => prev.filter(f => f.file_path !== filePath));
      if (stats) setStats({ ...stats, indexed_count: Math.max(0, stats.indexed_count - 1), vector_count: Math.max(0, stats.vector_count - 1) });
      toast('已删除索引', 'success');
    } catch { toast('删除失败', 'error'); } finally { setConfirmDeleteFile(null); }
  };

  return (
    <div className="flex flex-col h-full bg-slate-50">
      {/* Header */}
      <div className="bg-white border-b border-slate-200 px-6 py-3 flex items-center justify-between flex-shrink-0">
        <div className="flex gap-3">
          <div className="bg-indigo-50 rounded-lg px-4 py-2 text-center">
            <div className="text-xl font-bold text-indigo-600">{stats?.indexed_count ?? '-'}</div>
            <div className="text-xs text-slate-500">已索引</div>
          </div>
          <div className="bg-indigo-50 rounded-lg px-4 py-2 text-center">
            <div className="text-xl font-bold text-indigo-600">{stats?.vector_count ?? '-'}</div>
            <div className="text-xs text-slate-500">向量数</div>
          </div>
          {autoStatus && (
            <div className="bg-slate-50 rounded-lg px-4 py-2 text-center">
              <div className="text-xl font-bold text-slate-600">{autoStatus.indexed_files}</div>
              <div className="text-xs text-slate-500">自动清单</div>
            </div>
          )}
        </div>
        <div className="flex gap-2 items-center">
          {autoStatus && (
            <div className="flex items-center gap-2 mr-1">
              <span className="text-xs text-slate-500">
                自动索引{autoStatus.enabled ? ` · 每 ${autoStatus.interval}s${autoStatus.last_scan_time ? ` · 上次 ${autoStatus.last_scan_time}` : ''}${autoStatus.pending_changes > 0 ? ` · ${autoStatus.pending_changes} 待处理` : ''}` : '（已关闭）'}
              </span>
              <button onClick={handleToggleAuto} disabled={isToggling}
                className={`relative w-10 h-5.5 rounded-full transition-colors disabled:opacity-50 ${autoStatus.enabled ? 'bg-indigo-600' : 'bg-slate-300'}`}
                style={{ width: '40px', height: '22px' }}
                title={autoStatus.enabled ? '关闭' : '开启'}>
                <span className={`absolute top-0.5 left-0.5 w-4 h-4 rounded-full bg-white shadow transition-transform ${autoStatus.enabled ? 'translate-x-4.5' : ''}`}
                  style={{ transform: autoStatus.enabled ? 'translateX(18px)' : 'translateX(0)'}} />
              </button>
            </div>
          )}
          <button onClick={handleScanNow} disabled={isScanning}
            className="flex items-center gap-1.5 px-3 py-2 rounded-lg bg-indigo-600 text-white text-sm font-medium hover:bg-indigo-700 disabled:opacity-50 transition-colors">
            <Icon name="zap" size={15} />{isScanning ? '扫描中...' : '立即扫描'}
          </button>
          <button onClick={loadIndex}
            className="flex items-center gap-1.5 px-3 py-2 rounded-lg bg-slate-100 text-slate-600 text-sm font-medium hover:bg-slate-200 transition-colors">
            <Icon name="refresh" size={15} />刷新
          </button>
          <button onClick={() => setConfirmClear(true)} disabled={isClearing || (stats?.indexed_count ?? 0) === 0}
            className="flex items-center gap-1.5 px-3 py-2 rounded-lg bg-red-50 text-red-600 text-sm font-medium hover:bg-red-100 disabled:opacity-40 transition-colors">
            <Icon name="trash" size={15} />{isClearing ? '清空中...' : '清空'}
          </button>
        </div>
      </div>

      <div className="flex-1 overflow-y-auto p-6">
        {isLoading ? (
          <div className="flex items-center justify-center h-full text-slate-400">
            <div className="flex gap-1">
              <span className="w-2 h-2 bg-slate-400 rounded-full animate-bounce" style={{ animationDelay: '0ms' }} />
              <span className="w-2 h-2 bg-slate-400 rounded-full animate-bounce" style={{ animationDelay: '150ms' }} />
              <span className="w-2 h-2 bg-slate-400 rounded-full animate-bounce" style={{ animationDelay: '300ms' }} />
            </div>
          </div>
        ) : files.length === 0 ? (
          <div className="text-center py-16 text-slate-400">
            <Icon name="inbox" size={40} className="mx-auto text-slate-300 mb-3" />
            <div className="text-base">暂无索引文件</div>
            <div className="text-sm mt-1">开启自动索引或点击「立即扫描」</div>
          </div>
        ) : (
          <div className="space-y-3">
            <div className="text-sm text-slate-500">已索引文件 ({files.length})</div>
            <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-3">
              {files.map(file => (
                <div key={file.id} className="bg-white rounded-lg border border-slate-200 p-4 hover:shadow-card transition-shadow">
                  <div className="flex items-start justify-between gap-3 mb-2">
                    <div className="flex items-center gap-2">
                      <Icon name="file" size={18} className="text-slate-400" />
                      <div className="text-sm font-medium text-slate-700 truncate">{file.file_path.split(/[/\\]/).pop()}</div>
                    </div>
                    <button onClick={() => setConfirmDeleteFile(file.file_path)}
                      className="p-1.5 rounded-md text-slate-400 hover:text-red-600 hover:bg-red-50 transition-colors flex-shrink-0">
                      <Icon name="trash" size={15} />
                    </button>
                  </div>
                  <div className="text-xs text-slate-400 truncate mb-2">{file.file_path}</div>
                  <div className="text-xs text-slate-500 bg-slate-50 rounded-md p-2 line-clamp-2">{file.content_preview}</div>
                </div>
              ))}
            </div>
          </div>
        )}
      </div>

      <ConfirmDialog open={confirmClear} title="清空所有索引" message="确定要清空所有索引吗？此操作不可恢复。" onConfirm={handleClearIndex} onCancel={() => setConfirmClear(false)} danger />
      {confirmDeleteFile && (
        <ConfirmDialog open={!!confirmDeleteFile} title="删除索引" message={`确定要删除 "${confirmDeleteFile.split(/[/\\]/).pop()}" 的索引吗？`} onConfirm={() => handleDeleteFile(confirmDeleteFile)} onCancel={() => setConfirmDeleteFile(null)} danger />
      )}
    </div>
  );
}
