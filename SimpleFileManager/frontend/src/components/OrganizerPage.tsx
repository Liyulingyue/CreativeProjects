import { useState, useEffect } from 'react';
import { ApprovalCenter } from './Organizer/ApprovalCenter';
import { createPlan } from '../api';
import { useToast } from './ui/Toast';
import { Icon } from './ui/Icon';
import type { PlanActionType } from '../types';

interface Snapshot { date: string; file_count: number; files: number; dirs: number; }
interface FileChange { path: string; name: string; change_type: string; size: number; modified: string; }
interface Suggestion { id: string; type: string; priority: string; message: string; source_path: string | null; target_path: string | null; reason: string; }
interface CompareResult { date_from: string; date_to: string; added_files: FileChange[]; added_dirs: FileChange[]; deleted_files: FileChange[]; deleted_dirs: FileChange[]; suggestions: Suggestion[]; }

export function OrganizerPage() {
  const { toast } = useToast();
  const [snapshots, setSnapshots] = useState<Snapshot[]>([]);
  const [latestSnapshot, setLatestSnapshot] = useState<{ has_snapshot: boolean; date?: string; files?: number; dirs?: number } | null>(null);
  const [compareResult, setCompareResult] = useState<CompareResult | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [isTakingSnapshot, setIsTakingSnapshot] = useState(false);

  const loadSnapshots = async () => {
    setIsLoading(true);
    try {
      const [snapRes, latestRes] = await Promise.all([fetch('/api/organizer/snapshots'), fetch('/api/organizer/latest')]);
      if (snapRes.ok) setSnapshots((await snapRes.json()).snapshots || []);
      if (latestRes.ok) setLatestSnapshot(await latestRes.json());
    } catch (e) { console.error('Failed:', e); } finally { setIsLoading(false); }
  };

  useEffect(() => { loadSnapshots(); }, []);

  const handleTakeSnapshot = async () => {
    setIsTakingSnapshot(true);
    try { await fetch('/api/organizer/snapshot', { method: 'POST' }); await loadSnapshots(); toast('快照已拍摄', 'success'); }
    catch { toast('拍摄失败', 'error'); } finally { setIsTakingSnapshot(false); }
  };

  const handleCompare = async (dateFrom: string, dateTo: string) => {
    setIsLoading(true);
    try { const res = await fetch(`/api/organizer/compare?date_from=${dateFrom}&date_to=${dateTo}`); if (res.ok) setCompareResult(await res.json()); }
    catch { toast('对比失败', 'error'); } finally { setIsLoading(false); }
  };

  const handleSuggestionToPlan = async (sug: Suggestion) => {
    try {
      await createPlan({ title: sug.message, summary: sug.reason, source: 'suggestions',
        actions: [{ action_type: (sug.type === 'move' ? 'move' : 'create_folder') as PlanActionType, source_path: sug.source_path, target_path: sug.target_path, reason: sug.reason }] });
      toast('已生成整理计划，请在审批中心批准', 'success');
    } catch (e) { toast(e instanceof Error ? e.message : '生成失败', 'error'); }
  };

  const formatDate = (d: string) => new Date(d).toLocaleDateString('zh-CN', { month: 'short', day: 'numeric', year: 'numeric' });

  return (
    <div className="flex flex-col h-full bg-slate-50">
      <div className="bg-white border-b border-slate-200 px-6 py-3 flex items-center justify-between flex-shrink-0">
        <div className="flex items-center gap-3">
          <Icon name="clipboard" size={20} className="text-indigo-600" />
          <span className="text-base font-semibold text-slate-800">文件整理</span>
          {latestSnapshot?.has_snapshot && (
            <span className="text-sm text-slate-500">最新快照: {formatDate(latestSnapshot.date!)} ({latestSnapshot.files} 文件, {latestSnapshot.dirs} 目录)</span>
          )}
        </div>
        <button onClick={handleTakeSnapshot} disabled={isTakingSnapshot}
          className="flex items-center gap-1.5 px-3 py-2 rounded-lg bg-indigo-600 text-white text-sm font-medium hover:bg-indigo-700 disabled:opacity-50 transition-colors">
          <Icon name="camera" size={16} />{isTakingSnapshot ? '拍摄中...' : '拍摄快照'}
        </button>
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
        ) : (
          <div className="space-y-4">
            <ApprovalCenter />

            <div className="bg-white rounded-lg border border-slate-200 p-5">
              <div className="text-base font-semibold text-slate-700 mb-3">历史快照</div>
              {snapshots.length === 0 ? (
                <div className="text-center py-8 text-slate-400">
                  <Icon name="camera" size={32} className="mx-auto text-slate-300 mb-2" />
                  <div className="text-sm">暂无快照</div>
                  <div className="text-xs mt-1">点击「拍摄快照」开始记录文件结构</div>
                </div>
              ) : (
                <div className="space-y-1">
                  {snapshots.map((snap, idx) => (
                    <div key={snap.date} className="flex items-center justify-between p-2.5 rounded-lg hover:bg-slate-50 transition-colors">
                      <div className="flex items-center gap-3">
                        <div className="w-9 h-9 rounded-md bg-indigo-50 text-indigo-600 flex items-center justify-center text-sm font-semibold">{snap.file_count}</div>
                        <div>
                          <div className="text-sm font-medium text-slate-700">{formatDate(snap.date)}</div>
                          <div className="text-xs text-slate-400">{snap.files} 文件, {snap.dirs} 目录</div>
                        </div>
                      </div>
                      {idx < snapshots.length - 1 && (
                        <button onClick={() => handleCompare(snap.date, snapshots[idx + 1].date)}
                          className="px-3 py-1.5 rounded-lg bg-slate-100 text-slate-600 text-xs hover:bg-slate-200 transition-colors">对比变化</button>
                      )}
                    </div>
                  ))}
                </div>
              )}
            </div>

            {compareResult && (
              <div className="bg-white rounded-lg border border-slate-200 p-5">
                <div className="text-base font-semibold text-slate-700 mb-4">变化分析: {formatDate(compareResult.date_to)} vs {formatDate(compareResult.date_from)}</div>
                <div className="grid grid-cols-4 gap-3 mb-5">
                  <div className="bg-green-50 rounded-lg p-3 text-center"><div className="text-xl font-bold text-green-600">{compareResult.added_files.length}</div><div className="text-xs text-slate-500">新增文件</div></div>
                  <div className="bg-blue-50 rounded-lg p-3 text-center"><div className="text-xl font-bold text-blue-600">{compareResult.added_dirs.length}</div><div className="text-xs text-slate-500">新增目录</div></div>
                  <div className="bg-red-50 rounded-lg p-3 text-center"><div className="text-xl font-bold text-red-600">{compareResult.deleted_files.length}</div><div className="text-xs text-slate-500">删除文件</div></div>
                  <div className="bg-orange-50 rounded-lg p-3 text-center"><div className="text-xl font-bold text-orange-600">{compareResult.deleted_dirs.length}</div><div className="text-xs text-slate-500">删除目录</div></div>
                </div>
                {compareResult.suggestions.length > 0 && (
                  <div>
                    <div className="text-sm font-medium text-slate-600 mb-2 flex items-center gap-1.5"><Icon name="sparkles" size={16} className="text-indigo-500" />整理建议</div>
                    <div className="space-y-1.5">
                      {compareResult.suggestions.map(sug => (
                        <div key={sug.id} className="flex items-center gap-3 p-2.5 bg-slate-50 rounded-lg">
                          <div className={`w-8 h-8 rounded-md flex items-center justify-center ${sug.type === 'move' ? 'bg-blue-50 text-blue-600' : 'bg-orange-50 text-orange-600'}`}>
                            <Icon name={sug.type === 'move' ? 'move' : 'folder'} size={16} />
                          </div>
                          <div className="flex-1 min-w-0">
                            <div className="text-sm font-medium text-slate-700">{sug.message}</div>
                            <div className="text-xs text-slate-400 truncate">{sug.reason}</div>
                          </div>
                          {sug.type === 'move' && sug.source_path && sug.target_path && (
                            <button onClick={() => handleSuggestionToPlan(sug)}
                              className="px-3 py-1.5 rounded-lg bg-indigo-50 text-indigo-600 text-xs font-medium hover:bg-indigo-100 transition-colors whitespace-nowrap">转为计划</button>
                          )}
                        </div>
                      ))}
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
