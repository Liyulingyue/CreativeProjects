import { useEffect, useState, useCallback } from 'react';
import { fetchServiceHealth, fetchIndexStats } from '../api';
import { Icon } from './ui/Icon';

function formatBytes(bytes: number): string {
  if (bytes === 0) return '0 B';
  const k = 1024;
  const sizes = ['B', 'KB', 'MB', 'GB', 'TB'];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return parseFloat((bytes / Math.pow(k, i)).toFixed(1)) + ' ' + sizes[i];
}

export function ServiceStatus() {
  const [health, setHealth] = useState<{ llm: boolean; embedding: boolean } | null>(null);
  const [storage, setStorage] = useState<number | null>(null);

  const load = useCallback(async () => {
    try {
      const h = await fetchServiceHealth();
      setHealth({ llm: h.llm?.ok ?? false, embedding: h.embedding?.ok ?? false });
    } catch {
      setHealth(null);
    }
    try {
      const stats = await fetchIndexStats();
      setStorage(stats.storage_used);
    } catch {
      // ignore
    }
  }, []);

  useEffect(() => {
    load();
    const timer = setInterval(load, 60000);
    return () => clearInterval(timer);
  }, [load]);

  const dot = (ok: boolean) => (
    <span className={`w-1.5 h-1.5 rounded-full flex-shrink-0 ${ok ? 'bg-green-500' : 'bg-red-500'}`} />
  );

  return (
    <div className="px-3 pt-2 space-y-1.5">
      <div className="flex items-center gap-2 text-xs text-slate-400">
        {health ? (
          <>
            <span className="flex items-center gap-1" title={health.llm ? 'LLM 服务正常' : 'LLM 服务异常'}>
              {dot(health.llm)} LLM
            </span>
            <span className="flex items-center gap-1" title={health.embedding ? 'Embedding 服务正常' : 'Embedding 服务异常'}>
              {dot(health.embedding)} Embedding
            </span>
          </>
        ) : (
          <span className="text-slate-300">服务状态未知</span>
        )}
      </div>
      {storage !== null && (
        <div className="flex items-center gap-1.5 text-xs text-slate-400">
          <Icon name="hardDrive" size={12} />
          程序数据 {formatBytes(storage)}
        </div>
      )}
    </div>
  );
}
