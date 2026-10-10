import { useState } from 'react';
import { hybridSearch, streamRagQuery } from '../api';
import type { HybridSearchResult } from '../api';
import { Icon } from './ui/Icon';
import { useToast } from './ui/Toast';

type HybridSearchResponseLike = { results: HybridSearchResult[]; keyword_count: number; semantic_count: number; };

function SourceBadges({ sources }: { sources: string[] }) {
  return (
    <div className="flex gap-1">
      {sources.includes('keyword') && <span className="px-1.5 py-0.5 rounded bg-green-50 text-green-600 text-[10px] font-medium">关键词</span>}
      {sources.includes('semantic') && <span className="px-1.5 py-0.5 rounded bg-violet-50 text-violet-600 text-[10px] font-medium">语义</span>}
      {sources.length === 2 && <span className="px-1.5 py-0.5 rounded bg-indigo-50 text-indigo-600 text-[10px] font-medium">双路命中</span>}
    </div>
  );
}


/**
 * Renders the snippet with the backend's <<hit>> markers as real highlights.
 *
 * SQLite's snippet() returns the matched terms wrapped in << >>, which were
 * being drawn as literal text. Only the keyword channel produces them; semantic
 * hits come back as plain text and pass through unchanged.
 */
function HighlightedSnippet({ text }: { text: string }) {
  const parts = text.split(/(<<[^>]*>>)/g);
  return (
    <>
      {parts.map((part, i) =>
        part.startsWith('<<') && part.endsWith('>>') ? (
          <mark key={i} className="bg-amber-200 text-slate-900 rounded-sm px-0.5">
            {part.slice(2, -2)}
          </mark>
        ) : (
          <span key={i}>{part}</span>
        )
      )}
    </>
  );
}

export function SearchPage() {
  const { toast } = useToast();
  const [query, setQuery] = useState('');
  const [results, setResults] = useState<HybridSearchResponseLike | null>(null);
  const [isSearching, setIsSearching] = useState(false);
  const [hasSearched, setHasSearched] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const [aiAnswer, setAiAnswer] = useState<string | null>(null);
  const [isSummarizing, setIsSummarizing] = useState(false);
  const [aiPhase, setAiPhase] = useState<'idle' | 'searching' | 'writing'>('idle');

  const handleSearch = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!query.trim() || isSearching) return;
    setIsSearching(true); setHasSearched(true); setError(null);
    setResults(null); setAiAnswer(null);
    try {
      setResults(await hybridSearch(query, 10));
    } catch (err) {
      setError(err instanceof Error ? err.message : '未知错误');
    } finally { setIsSearching(false); }
  };

  const handleAiSummarize = async () => {
    if (!query.trim() || isSummarizing) return;
    setIsSummarizing(true); setAiAnswer(''); setAiPhase('searching');
    try {
      // Streamed: the model generates ~30 tok/s here, so the full answer takes
      // over a minute. The text appears as it is produced instead of arriving
      // all at once after an unresponsive spinner.
      await streamRagQuery(query, {
        onSearching: () => setAiPhase('searching'),
        onSources: () => setAiPhase('writing'),
        onToken: (t) => { setAiPhase('writing'); setAiAnswer(prev => (prev || '') + t); },
      }, 5);
    } catch (err) {
      toast('AI 总结失败: ' + (err instanceof Error ? err.message : '未知错误'), 'error');
    } finally { setIsSummarizing(false); setAiPhase('idle'); }
  };

  return (
    <div className="flex flex-col h-full bg-slate-50">
      {/* Search bar */}
      <div className="bg-white border-b border-slate-200 px-4 sm:px-6 py-3 sm:py-4 flex-shrink-0">
        <form onSubmit={handleSearch} className="flex items-center gap-2 sm:gap-3">
          <div className="relative flex-1 min-w-0">
            <span className="absolute left-3 top-1/2 -translate-y-1/2 text-slate-400"><Icon name="search" size={18} /></span>
            <input type="text" value={query} onChange={e => setQuery(e.target.value)}
              placeholder="搜索..."
              className="w-full pl-10 pr-3 sm:pr-4 py-2 sm:py-2.5 rounded-lg border border-slate-200 bg-slate-50 text-sm sm:text-base focus:outline-none focus:ring-2 focus:ring-indigo-400/30 focus:border-indigo-400 transition-all" />
          </div>
          <button type="submit" disabled={isSearching || !query.trim()}
            className="flex items-center gap-1.5 px-5 py-2.5 rounded-lg bg-indigo-600 text-white text-sm font-medium hover:bg-indigo-700 disabled:opacity-40 transition-colors whitespace-nowrap">
            {isSearching ? '搜索中...' : '搜索'}
          </button>
        </form>
      </div>

      {/* Results */}
      <div className="flex-1 overflow-y-auto">
        <div className="px-6 py-6">
          {!hasSearched && (
            <div className="text-center py-16 text-slate-400">
              <Icon name="search" size={40} className="mx-auto text-slate-300 mb-4" />
              <div className="text-base">输入关键词开始搜索</div>
              <div className="text-sm mt-2">BM25 关键词 + 向量语义双路召回，搜索后可点击「AI 总结」让 LLM 基于匹配文件生成回答</div>
            </div>
          )}

          {hasSearched && isSearching && (
            <div className="text-center py-16 text-slate-400">
              <div className="flex items-center justify-center gap-3">
                <div className="flex gap-1">
                  <span className="w-2 h-2 bg-slate-400 rounded-full animate-bounce" style={{ animationDelay: '0ms' }} />
                  <span className="w-2 h-2 bg-slate-400 rounded-full animate-bounce" style={{ animationDelay: '150ms' }} />
                  <span className="w-2 h-2 bg-slate-400 rounded-full animate-bounce" style={{ animationDelay: '300ms' }} />
                </div>
                <span>正在检索...</span>
              </div>
            </div>
          )}

          {hasSearched && !isSearching && error && (
            <div className="text-center py-16 text-red-500">
              <Icon name="alert" size={40} className="mx-auto text-red-300 mb-4" />
              <div className="text-base">搜索失败</div>
              <div className="text-sm mt-2">{error}</div>
            </div>
          )}

          {hasSearched && !isSearching && !error && results && (
            <div className="space-y-3">
              {/* Stats + AI button */}
              <div className="flex items-center justify-between gap-4">
                <div className="text-sm text-slate-500">
                  关键词命中 {results.keyword_count} 路 · 语义命中 {results.semantic_count} 路 · 融合后 {results.results.length} 条结果
                </div>
                {results.results.length > 0 && (
                  <button
                    onClick={handleAiSummarize}
                    disabled={isSummarizing}
                    className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg bg-violet-50 text-violet-600 text-sm font-medium hover:bg-violet-100 disabled:opacity-50 transition-colors whitespace-nowrap"
                  >
                    <Icon name="sparkles" size={16} />
                    {isSummarizing ? 'AI 总结中...' : 'AI 总结'}
                  </button>
                )}
              </div>

              {/* AI Answer */}
              {(isSummarizing || aiAnswer) && (
                <div className="bg-white rounded-lg shadow-card border border-violet-200 p-5">
                  <div className="flex items-center gap-2 mb-2 text-sm text-violet-600 font-medium">
                    <Icon name="sparkles" size={16} />
                    AI 回答
                    {isSummarizing && (
                      <span className="text-xs font-normal text-slate-400">
                        {aiPhase === 'searching' ? '正在检索相关片段...' : '正在逐字生成（本机约 30 字/秒）...'}
                      </span>
                    )}
                  </div>
                  {aiAnswer ? (
                    <div className="text-base leading-relaxed whitespace-pre-wrap text-slate-800">
                      {aiAnswer}
                      {isSummarizing && (
                        <span className="inline-block w-2 h-4 ml-0.5 bg-violet-400 animate-pulse align-text-bottom" />
                      )}
                    </div>
                  ) : (
                    <div className="flex items-center gap-2 text-slate-400 text-sm py-2">
                      <div className="flex gap-1">
                        <span className="w-1.5 h-1.5 bg-slate-400 rounded-full animate-bounce" style={{ animationDelay: '0ms' }} />
                        <span className="w-1.5 h-1.5 bg-slate-400 rounded-full animate-bounce" style={{ animationDelay: '150ms' }} />
                        <span className="w-1.5 h-1.5 bg-slate-400 rounded-full animate-bounce" style={{ animationDelay: '300ms' }} />
                      </div>
                      正在检索相关片段...
                    </div>
                  )}
                </div>
              )}

              {/* File results */}
              {results.results.length === 0 ? (
                <div className="text-center py-16 text-slate-400">
                  <Icon name="inbox" size={40} className="mx-auto text-slate-300 mb-4" />
                  <div className="text-base">未找到相关文件</div>
                  <div className="text-sm mt-2">先在「索引管理」页运行自动索引</div>
                </div>
              ) : results.results.map((r, idx) => (
                <div key={`${r.path}#${r.chunk_idx}`} className="bg-white rounded-lg shadow-card border border-slate-200 p-4">
                  <div className="flex items-center justify-between gap-3 mb-2">
                    <div className="flex items-center gap-3 min-w-0">
                      <div className="w-7 h-7 shrink-0 rounded-md bg-indigo-50 text-indigo-600 flex items-center justify-center text-xs font-semibold">{idx + 1}</div>
                      <div className="text-sm font-medium text-slate-700 truncate">{r.name}</div>
                      <span className="text-[10px] text-slate-400 font-mono flex-shrink-0">第 {r.chunk_idx} 块</span>
                      <SourceBadges sources={r.sources} />
                    </div>
                    <div className="text-xs text-slate-400 whitespace-nowrap">融合分 {r.score.toFixed(4)}</div>
                  </div>
                  <div className="text-xs text-slate-400 truncate mb-2">{r.path}</div>
                  <div className="text-xs text-slate-600 bg-slate-50 rounded-md p-2.5 whitespace-pre-wrap leading-relaxed">
                    <HighlightedSnippet text={r.preview} />
                    {typeof r.text_len === 'number' && r.text_len > 0 && (
                      <div className="mt-1.5 pt-1.5 border-t border-slate-200 text-[10px] text-slate-400">
                        本块约 {r.text_len} 字
                      </div>
                    )}
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
