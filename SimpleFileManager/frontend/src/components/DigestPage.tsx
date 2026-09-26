import { useCallback, useEffect, useState } from 'react';
import { fetchDigest, fetchDigestList, fetchLatestDigest, generateDigest } from '../api';
import type { Digest, DigestMeta } from '../api';
import { Icon } from './ui/Icon';

export function DigestPage() {
  const [latest, setLatest] = useState<Digest | null>(null);
  const [digestList, setDigestList] = useState<DigestMeta[]>([]);
  const [current, setCurrent] = useState<Digest | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [isGenerating, setIsGenerating] = useState(false);
  const [message, setMessage] = useState<string | null>(null);

  const load = useCallback(async () => {
    setIsLoading(true);
    try {
      const [latestData, listData] = await Promise.all([fetchLatestDigest(), fetchDigestList()]);
      setLatest(latestData.content ? latestData : null);
      setDigestList(listData);
      setCurrent(latestData.content ? latestData : null);
    } catch (e) { console.error('Failed:', e); } finally { setIsLoading(false); }
  }, []);

  useEffect(() => { load(); }, [load]);

  const handleGenerate = async () => {
    setIsGenerating(true); setMessage(null);
    try {
      const result = await generateDigest();
      if (result.success) { await load(); setCurrent(await fetchLatestDigest()); setMessage(`已生成 ${result.date} 日报（${result.files_count} 个文件）`); }
      else { setMessage(result.message || '生成失败'); }
    } catch (e) { setMessage(e instanceof Error ? e.message : '生成失败'); } finally { setIsGenerating(false); }
  };

  return (
    <div className="flex flex-col h-full bg-slate-50">
      <div className="bg-white border-b border-slate-200 px-6 py-3 flex items-center justify-between flex-shrink-0">
        <div className="flex items-center gap-3">
          <Icon name="newspaper" size={20} className="text-indigo-600" />
          <span className="text-base font-semibold text-slate-800">知识日报</span>
          <span className="text-xs text-slate-400">Agent 扫描今日新增/变动文件，自动提炼成结构化日报</span>
        </div>
        <button onClick={handleGenerate} disabled={isGenerating}
          className="flex items-center gap-1.5 px-3 py-2 rounded-lg bg-indigo-600 text-white text-sm font-medium hover:bg-indigo-700 disabled:opacity-50 transition-colors">
          <Icon name="sparkles" size={16} />{isGenerating ? '生成中...' : '生成今日日报'}
        </button>
      </div>

      {message && <div className="px-6 py-2 bg-indigo-50 text-indigo-700 text-sm">{message}</div>}

      <div className="flex-1 overflow-y-auto p-6">
        {isLoading ? (
          <div className="flex items-center justify-center h-full text-slate-400">加载中...</div>
        ) : !current ? (
          <div className="text-center py-16 text-slate-400">
            <Icon name="newspaper" size={40} className="mx-auto text-slate-300 mb-3" />
            <div className="text-base">还没有日报</div>
            <div className="text-sm mt-1">先运行自动索引，然后点击「生成今日日报」</div>
          </div>
        ) : (
          <div className="grid grid-cols-1 md:grid-cols-4 gap-4">
            <div className="lg:col-span-1">
              <div className="bg-white rounded-lg border border-slate-200 p-3">
                <div className="text-sm font-semibold text-slate-700 mb-2">历史日报</div>
                <div className="space-y-0.5">
                  {digestList.map(d => (
                    <button key={d.date} onClick={() => fetchDigest(d.date).then(setCurrent).catch(console.error)}
                      className={`w-full text-left px-3 py-2 rounded-lg text-sm transition-colors ${current.date === d.date ? 'bg-indigo-50 text-indigo-700 font-medium' : 'text-slate-600 hover:bg-slate-50'}`}>
                      <div>{d.date}</div>
                      <div className="text-xs text-slate-400">{d.files_count} 个文件</div>
                    </button>
                  ))}
                </div>
              </div>
            </div>
            <div className="lg:col-span-3">
              <div className="bg-white rounded-lg border border-slate-200 p-5">
                <div className="flex items-center justify-between mb-4 pb-3 border-b border-slate-100">
                  <div>
                    <div className="text-base font-semibold text-slate-800">知识日报 · {current.date}</div>
                    <div className="text-xs text-slate-400 mt-0.5">{current.files_count} 个文件 · {current.generated_by === 'llm' ? 'AI 撰写' : '基础清单（LLM 未配置）'}</div>
                  </div>
                  {latest?.date === current.date && <span className="px-2 py-0.5 rounded-full bg-green-50 text-green-600 text-xs font-medium">最新</span>}
                </div>
                <div className="text-sm leading-relaxed text-slate-700 whitespace-pre-wrap">{current.content}</div>
              </div>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
