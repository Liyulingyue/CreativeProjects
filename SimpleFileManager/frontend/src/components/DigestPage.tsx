import { useCallback, useEffect, useState } from 'react';
import { fetchDigest, fetchDigestList, fetchLatestDigest, generateDigest } from '../api';
import type { Digest, DigestMeta } from '../api';

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
    } catch (e) {
      console.error('Failed to load digests:', e);
    } finally {
      setIsLoading(false);
    }
  }, []);

  useEffect(() => {
    load();
  }, [load]);

  const handleGenerate = async () => {
    setIsGenerating(true);
    setMessage(null);
    try {
      const result = await generateDigest();
      if (result.success) {
        await load();
        const d = await fetchLatestDigest();
        setCurrent(d);
        setMessage(`已生成 ${result.date} 日报（${result.files_count} 个文件）`);
      } else {
        setMessage(result.message || '生成失败');
      }
    } catch (e) {
      setMessage(e instanceof Error ? e.message : '生成失败');
    } finally {
      setIsGenerating(false);
    }
  };

  const handleView = async (date: string) => {
    try {
      setCurrent(await fetchDigest(date));
    } catch (e) {
      console.error('Failed to fetch digest:', e);
    }
  };

  return (
    <div className="flex flex-col h-full bg-slate-50">
      {/* Header */}
      <div className="bg-white border-b border-slate-200 px-6 py-3 flex items-center justify-between">
        <div className="flex items-center gap-4">
          <div className="text-xl font-bold text-indigo-600">📰 知识日报</div>
          <div className="text-xs text-slate-400">Agent 扫描今日新增/变动文件，自动提炼成结构化日报</div>
        </div>
        <button
          onClick={handleGenerate}
          disabled={isGenerating}
          className="px-4 py-2 rounded-lg bg-indigo-600 text-white text-sm font-medium hover:bg-indigo-700 disabled:opacity-50 transition-colors"
        >
          {isGenerating ? '生成中...' : '生成今日日报'}
        </button>
      </div>

      {message && (
        <div className="px-6 py-2 bg-indigo-50 text-indigo-700 text-sm">{message}</div>
      )}

      {/* Content */}
      <div className="flex-1 overflow-y-auto p-6">
        {isLoading ? (
          <div className="flex items-center justify-center h-full text-slate-400">加载中...</div>
        ) : !current ? (
          <div className="text-center py-16 text-slate-400">
            <div className="text-5xl mb-4">📰</div>
            <div className="text-lg">还没有日报</div>
            <div className="text-sm mt-2">先在「索引管理」页运行自动索引，然后点击「生成今日日报」</div>
          </div>
        ) : (
          <div className="max-w-4xl mx-auto grid grid-cols-1 lg:grid-cols-4 gap-6">
            {/* Digest list */}
            <div className="lg:col-span-1">
              <div className="bg-white rounded-2xl border border-slate-200 p-4">
                <div className="text-sm font-semibold text-slate-700 mb-3">历史日报</div>
                <div className="space-y-1">
                  {digestList.map(d => (
                    <button
                      key={d.date}
                      onClick={() => handleView(d.date)}
                      className={`w-full text-left px-3 py-2 rounded-lg text-sm transition-colors ${
                        current.date === d.date
                          ? 'bg-indigo-50 text-indigo-700 font-medium'
                          : 'text-slate-600 hover:bg-slate-50'
                      }`}
                    >
                      <div>{d.date}</div>
                      <div className="text-xs text-slate-400">{d.files_count} 个文件</div>
                    </button>
                  ))}
                </div>
              </div>
            </div>

            {/* Digest content */}
            <div className="lg:col-span-3">
              <div className="bg-white rounded-2xl border border-slate-200 p-6">
                <div className="flex items-center justify-between mb-4 pb-4 border-b border-slate-100">
                  <div>
                    <div className="text-lg font-bold text-slate-800">知识日报 · {current.date}</div>
                    <div className="text-xs text-slate-400 mt-0.5">
                      {current.files_count} 个文件 ·
                      {current.generated_by === 'llm' ? ' AI 撰写' : ' 基础清单（LLM 未配置）'}
                    </div>
                  </div>
                  {latest?.date === current.date && (
                    <span className="px-2 py-0.5 rounded-full bg-green-100 text-green-700 text-xs">最新</span>
                  )}
                </div>
                <div className="text-sm leading-relaxed text-slate-700 whitespace-pre-wrap">
                  {current.content}
                </div>
              </div>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
