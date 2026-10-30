import { useCallback, useEffect, useState } from 'react';
import {
  cleanDuplicates, fetchDuplicates, scanForDuplicates, type DuplicateGroup,
} from '../api';
import { Icon } from './ui/Icon';

function formatSize(bytes: number): string {
  if (!bytes) return '-';
  const units = ['B', 'KB', 'MB', 'GB', 'TB'];
  let n = bytes;
  let i = 0;
  while (n >= 1024 && i < units.length - 1) { n /= 1024; i++; }
  return `${n < 10 && i > 0 ? n.toFixed(1) : Math.round(n)} ${units[i]}`;
}

/**
 * The files that are here more than once.
 *
 * A NAS accumulates copies: the same photo off the phone and off the
 * laptop, the same film downloaded twice, a document saved over and
 * over under a new name each time. The names do not have to match, so
 * these are found by what is inside them -- two files with the same
 * contents are the same file, whatever they are called.
 *
 * The point is the room: every copy but one is space being spent for
 * nothing. The list says how much, and leaves the decision about
 * which one to keep to somebody who knows what their files are.
 */
export function DuplicatesPage() {
  const [groups, setGroups] = useState<DuplicateGroup[]>([]);
  const [wasted, setWasted] = useState(0);
  const [isLoading, setIsLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [cleaning, setCleaning] = useState('');
  const [message, setMessage] = useState<string | null>(null);

  const load = useCallback(async () => {
    try {
      const data = await fetchDuplicates();
      setGroups(data.groups);
      setWasted(data.wasted);
    } catch (e) {
      setMessage(e instanceof Error ? e.message : '加载失败');
    } finally {
      setIsLoading(false);
    }
  }, []);

  useEffect(() => { load(); }, [load]);

  const rescan = async () => {
    setBusy(true); setMessage(null);
    try {
      const done = await scanForDuplicates();
      setMessage(done.remaining > 0
        ? `已计算 ${done.hashed} 个文件的指纹，还有 ${done.remaining} 个下次继续`
        : `已计算 ${done.hashed} 个文件的指纹`);
      await load();
    } catch (e) {
      setMessage(e instanceof Error ? e.message : '扫描失败');
    } finally {
      setBusy(false);
    }
  };

  // One copy is kept and the rest go to the recycle bin -- the bin,
  // not nowhere, so this can be undone.
  const clean = async (group: DuplicateGroup) => {
    if (!window.confirm(
      `这组有 ${group.copies} 份相同的文件。\n\n` +
      `保留一份，其余 ${group.copies - 1} 份放进回收站（可以随时还原）。\n\n继续？`
    )) return;
    setCleaning(group.hash); setMessage(null);
    try {
      const result = await cleanDuplicates(group.hash);
      setMessage(
        `已保留 ${result.kept.split('/').pop()}，${result.trashed.length} 份进了回收站，` +
        `腾出 ${formatSize(result.wasted)}`
      );
      await load();
    } catch (e) {
      setMessage(e instanceof Error ? e.message : '清理失败');
    } finally {
      setCleaning('');
    }
  };

  return (
    <div className="flex flex-col h-full bg-slate-50">
      <div className="bg-white border-b border-slate-200 px-4 sm:px-6 py-3 flex items-center gap-3 flex-shrink-0">
        <Icon name="copy" size={20} className="text-slate-500 flex-shrink-0" />
        <span className="text-base font-semibold text-slate-800 flex-shrink-0">重复文件</span>
        <span className="hidden sm:block text-xs text-slate-400 truncate">
          {isLoading
            ? '正在找...'
            : groups.length === 0
              ? '按文件内容判断，和文件名无关'
              : `${groups.length} 组 · 多占 ${formatSize(wasted)}`}
        </span>
      </div>

      <div className="flex-1 overflow-y-auto p-4 sm:p-6">
      <div className="space-y-4 max-w-4xl">
      {message && (
        <div className="text-sm text-indigo-700 bg-indigo-50 rounded-lg px-3 py-2">{message}</div>
      )}

      <div className="flex items-center gap-3">
        <div className="flex-1">
          <div className="text-sm text-slate-700">
            {isLoading ? '正在找重复的文件...' : (
              groups.length === 0
                ? '没有发现重复的文件'
                : `发现 ${groups.length} 组重复文件，占着 ${formatSize(wasted)}`
            )}
          </div>
          <div className="text-xs text-slate-400 mt-0.5">
            按文件内容判断，和文件名无关
          </div>
        </div>
        <button
          onClick={rescan}
          disabled={busy}
          className="px-3 py-2 rounded-lg bg-slate-100 text-slate-700 text-xs font-medium hover:bg-slate-200 disabled:opacity-50 transition-colors flex items-center gap-1.5"
        >
          <Icon name="refresh" size={14} />
          {busy ? '扫描中...' : '重新扫描'}
        </button>
      </div>

      {groups.length > 0 && (
        <div className="space-y-3">
          {groups.map(group => (
            <div key={group.hash} className="rounded-lg border border-slate-200 bg-white overflow-hidden">
              <div className="flex items-center gap-2 px-3 py-2 bg-slate-50 border-b border-slate-100">
                <Icon name="copy" size={15} className="text-amber-500 flex-shrink-0" />
                <span className="text-xs font-medium text-slate-700">
                  {group.copies} 份相同的文件
                </span>
                <span className="text-xs text-slate-400">
                  每个 {formatSize(group.size)}
                </span>
                <div className="flex-1" />
                <span className="text-xs font-medium text-amber-600">
                  多占 {formatSize(group.wasted)}
                </span>
                <button
                  onClick={() => clean(group)}
                  disabled={cleaning === group.hash}
                  className="px-2.5 py-1 rounded-lg bg-slate-100 text-slate-600 text-xs font-medium hover:bg-slate-200 disabled:opacity-50 transition-colors flex-shrink-0"
                >
                  {cleaning === group.hash ? '清理中...' : '保留一份，其余进回收站'}
                </button>
              </div>
              <div className="divide-y divide-slate-100">
                {group.files.map(file => (
                  <div key={file.path} className="flex items-center gap-2 px-3 py-2">
                    <Icon name="file" size={15} className="text-slate-400 flex-shrink-0" />
                    <span className="text-sm text-slate-700 truncate" title={file.path}>
                      {file.name}
                    </span>
                    <span className="text-xs text-slate-400 truncate flex-1" title={file.path}>
                      {file.path}
                    </span>
                    <span className="text-xs text-slate-400 flex-shrink-0">
                      {formatSize(file.size)}
                    </span>
                  </div>
                ))}
              </div>
            </div>
          ))}
        </div>
      )}
      </div>
      </div>
    </div>
  );
}
