import { useState } from 'react';

interface Source {
  file_path: string;
  score: number;
}

interface SearchResult {
  answer: string;
  sources: Source[];
}

export function SearchPage() {
  const [query, setQuery] = useState('');
  const [results, setResults] = useState<SearchResult | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [hasSearched, setHasSearched] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const handleSearch = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!query.trim() || isLoading) return;

    setIsLoading(true);
    setHasSearched(true);
    setError(null);
    setResults(null);

    try {
      const res = await fetch('/api/rag/query', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ question: query, top_k: 5 }),
      });

      if (!res.ok) throw new Error('请求失败，请稍后重试');

      const data: SearchResult = await res.json();
      setResults(data);
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
          <form onSubmit={handleSearch} className="flex items-center gap-4">
            <div className="text-2xl font-bold text-indigo-600 whitespace-nowrap">🔍 搜索</div>
            <input
              type="text"
              value={query}
              onChange={e => setQuery(e.target.value)}
              placeholder="输入内容..."
              className="flex-1 px-5 py-3 rounded-xl border border-slate-200 bg-slate-50 text-lg focus:outline-none focus:ring-2 focus:ring-indigo-500 focus:border-transparent"
            />
            <button
              type="submit"
              disabled={isLoading || !query.trim()}
              className="px-8 py-3 rounded-xl bg-indigo-600 text-white text-lg font-medium hover:bg-indigo-700 disabled:opacity-50 disabled:cursor-not-allowed transition-colors whitespace-nowrap"
            >
              {isLoading ? '搜索中...' : '搜索'}
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
              <div className="text-sm mt-2">在文件内容中搜索相关信息</div>
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
                <span>正在搜索...</span>
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

          {hasSearched && !isLoading && results && (
            <div className="space-y-6">
              {/* Answer */}
              <div className="bg-white rounded-2xl shadow-sm border border-slate-200 p-6">
                <div className="text-sm text-slate-500 mb-3">搜索结果</div>
                <div className="text-lg leading-relaxed whitespace-pre-wrap">
                  {results.answer}
                </div>
              </div>

              {/* Sources */}
              {results.sources && results.sources.length > 0 && (
                <div className="bg-white rounded-2xl shadow-sm border border-slate-200 p-6">
                  <div className="text-sm text-slate-500 mb-4">
                    参考文档 ({results.sources.length})
                  </div>
                  <div className="space-y-3">
                    {results.sources.map((source, idx) => (
                      <div
                        key={idx}
                        className="flex items-center gap-4 p-3 rounded-xl hover:bg-slate-50 transition-colors"
                      >
                        <div className="w-8 h-8 rounded-lg bg-indigo-100 text-indigo-600 flex items-center justify-center text-sm font-bold">
                          {idx + 1}
                        </div>
                        <div className="flex-1 min-w-0">
                          <div className="text-sm font-medium text-slate-700 truncate">
                            {source.file_path.split(/[/\\]/).pop()}
                          </div>
                          <div className="text-xs text-slate-400 truncate">
                            {source.file_path}
                          </div>
                        </div>
                        <div className="text-xs text-slate-400">
                          相似度 {Math.round(source.score * 100)}%
                        </div>
                      </div>
                    ))}
                  </div>
                </div>
              )}

              {results.sources && results.sources.length === 0 && (
                <div className="text-center py-8 text-slate-400 text-sm">
                  未找到相关参考文档
                </div>
              )}
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
