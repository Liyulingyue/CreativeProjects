import { useState } from 'react';
import { hybridSearch } from '../api';
import type { HybridSearchResult } from '../api';

interface Source {
  file_path: string;
  score: number;
}

interface AiResult {
  answer: string;
  sources: Source[];
}

type HybridSearchResponseLike = {
  results: HybridSearchResult[];
  keyword_count: number;
  semantic_count: number;
};

function SourceBadges({ sources }: { sources: string[] }) {
  return (
    <div className="flex gap-1">
      {sources.includes('keyword') && (
        <span className="px-1.5 py-0.5 rounded bg-green-100 text-green-700 text-xs">关键词</span>
      )}
      {sources.includes('semantic') && (
        <span className="px-1.5 py-0.5 rounded bg-purple-100 text-purple-700 text-xs">语义</span>
      )}
      {sources.length === 2 && (
        <span className="px-1.5 py-0.5 rounded bg-indigo-100 text-indigo-700 text-xs">双路命中</span>
      )}
    </div>
  );
}

export function SearchPage() {
  const [query, setQuery] = useState('');
  const [mode, setMode] = useState<'hybrid' | 'ai'>('hybrid');
  const [hybridResults, setHybridResults] = useState<HybridSearchResponseLike | null>(null);
  const [aiResults, setAiResults] = useState<AiResult | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [hasSearched, setHasSearched] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const handleSearch = async (e: React.FormEvent, searchMode: 'hybrid' | 'ai') => {
    e.preventDefault();
    if (!query.trim() || isLoading) return;

    setIsLoading(true);
    setHasSearched(true);
    setError(null);
    setHybridResults(null);
    setAiResults(null);

    try {
      if (searchMode === 'hybrid') {
        const data = await hybridSearch(query, 10);
        setHybridResults(data);
      } else {
        const res = await fetch('/api/rag/query', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ question: query, top_k: 5 }),
        });
        if (!res.ok) throw new Error('请求失败，请稍后重试');
        setAiResults(await res.json());
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : '未知错误');
    } finally {
      setIsLoading(false);
    }
  };

  return (
    <div className="flex flex-col h-full bg-slate-50">
      {/* Search Header */}
      <div className="bg-white border-b border-slate-200 px-6 py-4">
        <div className="max-w-4xl mx-auto">
          <form onSubmit={e => handleSearch(e, mode)} className="flex items-center gap-4">
            <div className="text-2xl font-bold text-indigo-600 whitespace-nowrap">🔍 搜索</div>
            <div className="flex rounded-xl overflow-hidden border border-slate-200">
              <button
                type="button"
                onClick={() => setMode('hybrid')}
                className={`px-4 py-3 text-sm font-medium transition-colors ${
                  mode === 'hybrid' ? 'bg-indigo-600 text-white' : 'bg-slate-50 text-slate-600 hover:bg-slate-100'
                }`}
              >
                混合检索
              </button>
              <button
                type="button"
                onClick={() => setMode('ai')}
                className={`px-4 py-3 text-sm font-medium transition-colors ${
                  mode === 'ai' ? 'bg-indigo-600 text-white' : 'bg-slate-50 text-slate-600 hover:bg-slate-100'
                }`}
              >
                AI 问答
              </button>
            </div>
            <input
              type="text"
              value={query}
              onChange={e => setQuery(e.target.value)}
              placeholder={mode === 'hybrid' ? '关键词或自然语言...' : '用自然语言提问...'}
              className="flex-1 px-5 py-3 rounded-xl border border-slate-200 bg-slate-50 text-lg focus:outline-none focus:ring-2 focus:ring-indigo-500 focus:border-transparent"
            />
            <button
              type="submit"
              disabled={isLoading || !query.trim()}
              className="px-8 py-3 rounded-xl bg-indigo-600 text-white text-lg font-medium hover:bg-indigo-700 disabled:opacity-50 disabled:cursor-not-allowed transition-colors whitespace-nowrap"
            >
              {isLoading ? '搜索中...' : mode === 'hybrid' ? '搜索' : '提问'}
            </button>
          </form>
        </div>
      </div>

      {/* Results */}
      <div className="flex-1 overflow-y-auto">
        <div className="max-w-4xl mx-auto px-6 py-6">
          {!hasSearched && (
            <div className="text-center py-16 text-slate-400">
              <div className="text-5xl mb-4">🔍</div>
              <div className="text-lg">输入关键词开始搜索</div>
              <div className="text-sm mt-2">
                混合检索 = BM25 关键词 + 向量语义双路召回，AI 问答 = LLM 基于文件内容回答
              </div>
            </div>
          )}

          {hasSearched && isLoading && (
            <div className="text-center py-16 text-slate-400">
              <div className="flex items-center justify-center gap-3">
                <div className="flex gap-1">
                  <span className="w-3 h-3 bg-slate-400 rounded-full animate-bounce" style={{ animationDelay: '0ms' }} />
                  <span className="w-3 h-3 bg-slate-400 rounded-full animate-bounce" style={{ animationDelay: '150ms' }} />
                  <span className="w-3 h-3 bg-slate-400 rounded-full animate-bounce" style={{ animationDelay: '300ms' }} />
                </div>
                <span>{mode === 'hybrid' ? '正在检索...' : 'AI 正在阅读文件...'}</span>
              </div>
            </div>
          )}

          {hasSearched && !isLoading && error && (
            <div className="text-center py-16 text-red-400">
              <div className="text-5xl mb-4">❌</div>
              <div className="text-lg">搜索失败</div>
              <div className="text-sm mt-2">{error}</div>
            </div>
          )}

          {/* Hybrid results */}
          {hasSearched && !isLoading && !error && hybridResults && (
            <div className="space-y-3">
              <div className="text-sm text-slate-500">
                关键词命中 {hybridResults.keyword_count} 路 · 语义命中 {hybridResults.semantic_count} 路 ·
                融合后 {hybridResults.results.length} 条结果
              </div>
              {hybridResults.results.length === 0 ? (
                <div className="text-center py-16 text-slate-400">
                  <div className="text-5xl mb-4">📭</div>
                  <div className="text-lg">未找到相关文件</div>
                  <div className="text-sm mt-2">先在「索引管理」页运行自动索引</div>
                </div>
              ) : (
                hybridResults.results.map((r, idx) => (
                  <div key={r.path} className="bg-white rounded-2xl shadow-sm border border-slate-200 p-5">
                    <div className="flex items-center justify-between gap-3 mb-2">
                      <div className="flex items-center gap-3 min-w-0">
                        <div className="w-8 h-8 shrink-0 rounded-lg bg-indigo-100 text-indigo-600 flex items-center justify-center text-sm font-bold">
                          {idx + 1}
                        </div>
                        <div className="text-sm font-medium text-slate-700 truncate">{r.name}</div>
                        <SourceBadges sources={r.sources} />
                      </div>
                      <div className="text-xs text-slate-400 whitespace-nowrap">
                        融合分 {r.score.toFixed(4)}
                      </div>
                    </div>
                    <div className="text-xs text-slate-400 truncate mb-2">{r.path}</div>
                    <div className="text-xs text-slate-500 bg-slate-50 rounded-lg p-2 line-clamp-2 whitespace-pre-wrap">
                      {r.preview}
                    </div>
                  </div>
                ))
              )}
            </div>
          )}

          {/* AI answer results */}
          {hasSearched && !isLoading && !error && aiResults && (
            <div className="space-y-6">
              <div className="bg-white rounded-2xl shadow-sm border border-slate-200 p-6">
                <div className="text-sm text-slate-500 mb-3">AI 回答</div>
                <div className="text-lg leading-relaxed whitespace-pre-wrap">{aiResults.answer}</div>
              </div>

              {aiResults.sources && aiResults.sources.length > 0 && (
                <div className="bg-white rounded-2xl shadow-sm border border-slate-200 p-6">
                  <div className="text-sm text-slate-500 mb-4">参考文档 ({aiResults.sources.length})</div>
                  <div className="space-y-3">
                    {aiResults.sources.map((source, idx) => (
                      <div key={idx} className="flex items-center gap-4 p-3 rounded-xl hover:bg-slate-50 transition-colors">
                        <div className="w-8 h-8 rounded-lg bg-indigo-100 text-indigo-600 flex items-center justify-center text-sm font-bold">
                          {idx + 1}
                        </div>
                        <div className="flex-1 min-w-0">
                          <div className="text-sm font-medium text-slate-700 truncate">
                            {source.file_path.split(/[/\\]/).pop()}
                          </div>
                          <div className="text-xs text-slate-400 truncate">{source.file_path}</div>
                        </div>
                        <div className="text-xs text-slate-400">相似度 {Math.round(source.score * 100)}%</div>
                      </div>
                    ))}
                  </div>
                </div>
              )}

              {aiResults.sources && aiResults.sources.length === 0 && (
                <div className="text-center py-8 text-slate-400 text-sm">未找到相关参考文档</div>
              )}
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
