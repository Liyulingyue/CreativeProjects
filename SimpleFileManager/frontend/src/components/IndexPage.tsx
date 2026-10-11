import { useState, useEffect, useCallback, useRef } from 'react';
import { fetchAutoIndexStatus, fetchIndexSkipped, pauseAutoIndex, resumeAutoIndex, runAutoIndexNow, updateSettings } from '../api';
import { authFetch } from '../auth';
import { useToast } from './ui/Toast';
import { ConfirmDialog } from './ui/Dialog';
import { Icon } from './ui/Icon';
import type { AutoIndexStatus } from '../types';
import type { IndexSkipped } from '../api';

interface IndexedFile {
  id: string;
  file_path: string;
  content_preview: string;
  chunk_count: number;
  total_chunks: number;
  percent: number;
}
interface IndexStats {
  indexed_count: number;
  vector_count: number;
  total_chunks: number;
  semantic_percent: number;
}



/**
 * Background-work schedule, modelled on Windows Update's active hours: pick a
 * window during which indexing may run, or pause it for a while. Indexing is
 * deferrable, so keeping it out of the hours you are actually using the machine
 * is the cheapest way to reclaim the cores.
 */
function ScheduleControl({ status, onPause, onResume, onChangeHours }: {
  status: AutoIndexStatus;
  onPause: (seconds: number) => void;
  onResume: () => void;
  onChangeHours: (window: string) => void;
}) {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);
  const [start, setStart] = useState('23:00');
  const [end, setEnd] = useState('07:00');

  useEffect(() => {
    if (!open) return;
    const close = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) setOpen(false);
    };
    document.addEventListener('mousedown', close);
    return () => document.removeEventListener('mousedown', close);
  }, [open]);

  const paused = status.paused_remaining !== 0 || status.paused_indefinitely;
  const scheduled = status.active_hours && status.active_hours !== 'all';

  const label = paused
    ? status.paused_indefinitely ? '已暂停' : `${Math.ceil(status.paused_remaining / 60)} 分钟后恢复`
    : scheduled
      ? (status.in_active_hours ? '时段内运行' : '等待时段')
      : '全天运行';

  const tone = paused ? 'bg-amber-50 text-amber-700 hover:bg-amber-100'
    : scheduled && !status.in_active_hours ? 'bg-slate-100 text-slate-500 hover:bg-slate-200'
    : 'bg-slate-100 text-slate-600 hover:bg-slate-200';

  return (
    <div className="relative" ref={ref}>
      <button onClick={() => setOpen(v => !v)}
        className={`flex items-center gap-1.5 px-3 py-2 rounded-lg text-sm font-medium transition-colors ${tone}`}>
        <Icon name={paused ? 'clock' : scheduled ? 'clock' : 'zap'} size={15} />
        {label}
        <Icon name="chevronDown" size={12} />
      </button>

      {open && (
        <div className="absolute right-0 top-full mt-1 z-30 bg-white border border-slate-200 rounded-lg shadow-popover p-3 w-[calc(100vw-2rem)] max-w-[268px] space-y-3">
          <div>
            <div className="text-xs font-medium text-slate-500 mb-1.5">稍后恢复</div>
            <div className="flex flex-wrap gap-1.5">
              {paused ? (
                <button onClick={() => { onResume(); setOpen(false); }}
                  className="px-2.5 py-2.5 rounded-lg bg-indigo-600 text-white text-xs font-medium hover:bg-indigo-700">
                  立即恢复
                </button>
              ) : (
                <>
                  {[300, 1800].map(sec => (
                    <button key={sec} onClick={() => { onPause(sec); setOpen(false); }}
                      className="px-2.5 py-2.5 rounded-lg border border-slate-200 text-xs text-slate-600 hover:border-indigo-300 hover:text-indigo-600">
                      {sec / 60} 分钟后
                    </button>
                  ))}
                  <button onClick={() => { onPause(0); setOpen(false); }}
                    className="px-2.5 py-2.5 rounded-lg border border-slate-200 text-xs text-slate-600 hover:border-indigo-300 hover:text-indigo-600">
                    一直暂停
                  </button>
                </>
              )}
            </div>
          </div>

          <div className="border-t border-slate-100 pt-3">
            <div className="text-xs font-medium text-slate-500 mb-1.5">运行时段</div>
            <div className="space-y-1.5">
              <label className="flex items-center gap-2 py-2 text-xs text-slate-600 cursor-pointer">
                <input type="radio" checked={status.active_hours === 'all'}
                  onChange={() => onChangeHours('all')} className="accent-indigo-600" />
                全天运行
              </label>
              <label className="flex items-center gap-2 py-2 text-xs text-slate-600 cursor-pointer">
                <input type="radio" checked={status.active_hours !== 'all'}
                  onChange={() => onChangeHours(`${start}-${end}`)} className="accent-indigo-600" />
                仅在指定时段
              </label>
              {status.active_hours !== 'all' && (
                <div className="flex flex-wrap items-center gap-1.5 pl-5">
                  <input type="time" value={start}
                    onChange={(e) => { setStart(e.target.value); onChangeHours(`${e.target.value}-${end}`); }}
                    className="px-2 py-2 rounded border border-slate-200 text-base sm:text-xs" />
                  <span className="text-slate-400 text-xs">至</span>
                  <input type="time" value={end}
                    onChange={(e) => { setEnd(e.target.value); onChangeHours(`${start}-${e.target.value}`); }}
                    className="px-2 py-2 rounded border border-slate-200 text-base sm:text-xs" />
                </div>
              )}
              {status.active_hours !== 'all' && !status.in_active_hours && (
                <div className="pl-5 text-[11px] text-slate-400">当前不在时段内，索引会等到时段开始</div>
              )}
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

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
  const [skipped, setSkipped] = useState<IndexSkipped | null>(null);
  const [showSkipped, setShowSkipped] = useState(false);

  const handlePause = async (seconds: number) => {
    try { await pauseAutoIndex(seconds); await loadAutoStatus(); }
    catch (e) { toast('暂停失败: ' + (e instanceof Error ? e.message : '未知错误'), 'error'); }
  };
  const handleResume = async () => {
    try { await resumeAutoIndex(); await loadAutoStatus(); }
    catch (e) { toast('恢复失败: ' + (e instanceof Error ? e.message : '未知错误'), 'error'); }
  };

  const loadAutoStatus = useCallback(async () => {
    try { setAutoStatus(await fetchAutoIndexStatus()); } catch (e) { console.error('Failed:', e); }
  }, []);

  // Skip counts are populated by a scan, so refresh them alongside the scan
  // status rather than only on first mount.
  const loadSkipped = useCallback(async () => {
    try { setSkipped(await fetchIndexSkipped()); } catch { /* non-fatal */ }
  }, []);

  const loadIndex = async () => {
    setIsLoading(true);
    try {
      const [statsRes, filesRes] = await Promise.all([authFetch('/api/rag/status'), authFetch('/api/rag/files')]);
      if (statsRes.ok) setStats(await statsRes.json());
      if (filesRes.ok) setFiles((await filesRes.json()).files || []);
    } catch (e) { console.error('Failed:', e); } finally { setIsLoading(false); }
  };

  useEffect(() => {
    loadIndex(); loadAutoStatus(); loadSkipped();
    const timer = setInterval(() => { loadAutoStatus(); loadSkipped(); }, 10000);
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
    try {
      // The scan runs in the background now, so this returns immediately:
      // polling is what actually reports progress.
      await runAutoIndexNow();
      toast('扫描已开始，后台运行', 'success');
      await loadAutoStatus();
      await loadSkipped();
    }
    catch { toast('扫描失败', 'error'); } finally { setIsScanning(false); }
  };

  const handleClearIndex = async () => {
    setIsClearing(true);
    try { await authFetch('/api/rag/clear', { method: 'DELETE' }); setFiles([]); setStats({ indexed_count: 0, vector_count: 0, total_chunks: 0, semantic_percent: 0 }); toast('索引已清空', 'success'); }
    catch { toast('清空失败', 'error'); } finally { setIsClearing(false); setConfirmClear(false); }
  };

  const handleDeleteFile = async (filePath: string) => {
    try {
      await authFetch(`/api/rag/files/${encodeURIComponent(filePath)}`, { method: 'DELETE' });
      setFiles(prev => prev.filter(f => f.file_path !== filePath));
      if (stats) setStats({ ...stats, indexed_count: Math.max(0, stats.indexed_count - 1), vector_count: Math.max(0, stats.vector_count - 1) });
      toast('已删除索引', 'success');
    } catch { toast('删除失败', 'error'); } finally { setConfirmDeleteFile(null); }
  };

  return (
    <div className="flex flex-col h-full bg-slate-50">
      {/* Header */}
      <div className="bg-white border-b border-slate-200 px-4 sm:px-6 py-3 flex items-center justify-between flex-wrap gap-2 flex-shrink-0">
        <div className="grid grid-cols-2 gap-2 sm:flex sm:gap-3">
          <div className="bg-indigo-50 rounded-lg px-4 py-2 text-center">
            <div className="text-xl font-bold text-indigo-600">{stats?.indexed_count ?? '-'}</div>
            <div className="text-xs text-slate-500">已索引</div>
          </div>
          <div
            className="bg-indigo-50 rounded-lg px-4 py-2 text-center"
            title={`已向量化 ${stats?.vector_count ?? 0} 块，共 ${stats?.total_chunks ?? 0} 块`}
          >
            <div className="text-xl font-bold text-indigo-600">{stats?.semantic_percent ?? 0}%</div>
            <div className="text-xs text-slate-500">
              语义覆盖 {stats?.vector_count ?? 0}/{stats?.total_chunks ?? 0}
            </div>
          </div>
          {autoStatus && (
            <div className="bg-slate-50 rounded-lg px-4 py-2 text-center">
              <div className="text-xl font-bold text-slate-600">{autoStatus.indexed_files}</div>
              <div className="text-xs text-slate-500">自动清单</div>
            </div>
          )}
          {skipped && (skipped.unsupported.length > 0 || skipped.failed.length > 0 || skipped.oversized.length > 0) && (
            <button
              onClick={() => setShowSkipped(v => !v)}
              className={`bg-amber-50 rounded-lg px-4 py-2 text-center transition-colors hover:bg-amber-100 ${
                showSkipped ? 'ring-1 ring-amber-300' : ''
              }`}
              title="查看未索引的文件"
            >
              <div className="text-xl font-bold text-amber-600">
                {skipped.unsupported.length + skipped.failed.length + skipped.oversized.length}
              </div>
              <div className="text-xs text-amber-700 flex items-center gap-1">
                未索引
                <Icon name={showSkipped ? 'chevronDown' : 'chevronRight'} size={11} />
              </div>
            </button>
          )}
        </div>
        <div className="flex gap-2 items-center flex-wrap">
          {autoStatus && (
            <div className="flex items-center gap-2 mr-1 min-w-0 flex-1 sm:flex-none">
              <span className="text-xs text-slate-500 truncate">
                自动索引{autoStatus.enabled ? ` · 每 ${autoStatus.interval}s${autoStatus.last_scan_time ? ` · 上次 ${autoStatus.last_scan_time}` : ''}${autoStatus.pending_changes > 0 ? ` · ${autoStatus.pending_changes} 待处理` : ''}` : '（已关闭）'}
                {autoStatus.yielded_for_interaction && (
                  <span className="ml-1 text-indigo-500" title="检测到你正在使用对话或搜索，索引已暂停让出算力，闲置后自动继续">
                    · 已为交互让路
                  </span>
                )}
              </span>
              <button onClick={handleToggleAuto} disabled={isToggling}
                className={`relative w-10 h-[22px] rounded-full transition-colors disabled:opacity-50
                  after:content-[''] after:absolute after:-inset-y-2.5 after:-inset-x-2.5 ${autoStatus.enabled ? 'bg-indigo-600' : 'bg-slate-300'}`}
                style={{ width: '40px', height: '22px' }}
                title={autoStatus.enabled ? '关闭' : '开启'}>
                <span className={`absolute top-0.5 left-0.5 w-4 h-4 rounded-full bg-white shadow transition-transform ${autoStatus.enabled ? 'translate-x-[18px]' : 'translate-x-0'}`}
                  style={{ transform: autoStatus.enabled ? 'translateX(18px)' : 'translateX(0)'}} />
              </button>
            </div>
          )}
                    {autoStatus && (
            <ScheduleControl
              status={autoStatus}
              onPause={handlePause}
              onResume={handleResume}
              onChangeHours={(w) => {
                updateSettings({ index_active_hours: w }).then(() => loadAutoStatus()).catch(
                  (e) => toast('保存失败: ' + (e instanceof Error ? e.message : '未知错误'), 'error'));
              }}
            />
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
        {showSkipped && skipped && (
          <div className="bg-white rounded-lg border border-amber-200 mb-4 overflow-hidden">
            <div className="px-4 py-3 border-b border-amber-100 bg-amber-50/60">
              <div className="text-sm font-medium text-slate-700">未索引的文件</div>
              <div className="text-xs text-slate-500 mt-0.5">
                这些文件已正常保存在存储中，但内容无法进入检索，因此搜索时不会命中。
              </div>
            </div>

            {skipped.unsupported.length > 0 && (
              <div className="border-b border-slate-100">
                <div className="px-4 py-2 text-xs font-medium text-slate-500 bg-slate-50">
                  格式不支持（{skipped.unsupported.length}）
                </div>
                <div className="max-h-48 overflow-y-auto divide-y divide-slate-50">
                  {skipped.unsupported.map(f => (
                    <div key={f.path} className="px-4 py-1.5 flex items-center gap-2 text-xs">
                      <code className="text-slate-400 font-mono flex-shrink-0">{f.ext || '无扩展名'}</code>
                      <span className="text-slate-600 truncate">{f.path}</span>
                    </div>
                  ))}
                </div>
              </div>
            )}


            {skipped.oversized.length > 0 && (
              <div className="border-t border-slate-100">
                <div className="px-4 py-2 text-xs font-medium text-slate-500 bg-slate-50">
                  超过大小限制（{skipped.oversized.length}）
                </div>
                <div className="max-h-48 overflow-y-auto divide-y divide-slate-50">
                  {skipped.oversized.map(f => (
                    <div key={f.path} className="px-4 py-1.5 flex items-center gap-2 text-xs">
                      <span className="text-slate-400 flex-shrink-0">{f.size}</span>
                      <span className="text-slate-600 truncate">{f.path}</span>
                    </div>
                  ))}
                </div>
              </div>
            )}

            {skipped.failed.length > 0 && (
              <div>
                <div className="px-4 py-2 text-xs font-medium text-slate-500 bg-slate-50">
                  解析失败（{skipped.failed.length}）
                </div>
                <div className="max-h-48 overflow-y-auto divide-y divide-slate-50">
                  {skipped.failed.map(f => (
                    <div key={f.path} className="px-4 py-1.5 text-xs">
                      <div className="text-slate-600 truncate">{f.path}</div>
                      <div className="text-amber-600 mt-0.5">{f.reason}</div>
                    </div>
                  ))}
                </div>
              </div>
            )}
          </div>
        )}

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
            <div className="text-xs text-slate-400 -mt-2">
              文件一旦入库即可被关键词搜索；每张卡片下方的进度条表示语义向量化推进到哪一块，可随时离开，稍后回来继续。搜索结果会标出命中的块号。
            </div>
            <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-3">
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
                  {/* Vectorisation is incremental, so say plainly which part of
                      this file is embedded and which is still queued. */}
                  <div className="flex items-center gap-2 mb-2">
                    <div className="flex-1 h-1.5 rounded-full bg-slate-100 overflow-hidden">
                      <div
                        className={`h-full rounded-full transition-all ${
                          file.percent >= 100 ? 'bg-green-500' : 'bg-indigo-500'
                        }`}
                        style={{ width: `${file.percent}%` }}
                      />
                    </div>
                    <span className={`text-[11px] tabular-nums flex-shrink-0 ${
                      file.percent >= 100 ? 'text-green-600' : 'text-slate-400'
                    }`}>
                      {file.chunk_count}/{file.total_chunks} 块
                      {file.percent >= 100 ? ' ✓' : ` · ${file.percent}%`}
                    </span>
                  </div>
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
