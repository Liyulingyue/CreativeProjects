import { useState, useEffect } from 'react'
import VocabCard from '../components/VocabCard'

interface Vocabulary {
  id: number
  word: string
  phonetic: string | null
  part_of_speech: string | null
  definition: string
  unit: string | null
}

interface VocabResponse {
  items: Vocabulary[]
  total: number
  page: number
  page_size: number
  total_pages: number
}

export default function Vocab() {
  const [vocabList, setVocabList] = useState<Vocabulary[]>([])
  const [page, setPage] = useState(1)
  const [totalPages, setTotalPages] = useState(1)
  const [total, setTotal] = useState(0)
  const [search, setSearch] = useState('')
  const [searchResult, setSearchResult] = useState<Vocabulary[]>([])
  const [isSearching, setIsSearching] = useState(false)
  const [loading, setLoading] = useState(false)

  useEffect(() => {
    if (!isSearching) {
      fetchVocab()
    }
  }, [page, isSearching])

  const fetchVocab = async () => {
    setLoading(true)
    try {
      const res = await fetch(`/api/v1/vocabulary?page=${page}&page_size=50`)
      const data: VocabResponse = await res.json()
      setVocabList(data.items)
      setTotalPages(data.total_pages)
      setTotal(data.total)
    } catch (e) {
      console.error('Failed to fetch vocabulary:', e)
    }
    setLoading(false)
  }

  const handleSearch = async () => {
    if (!search.trim()) {
      setIsSearching(false)
      return
    }
    setIsSearching(true)
    setPage(1)
    try {
      const res = await fetch(`/api/v1/vocabulary/search?q=${encodeURIComponent(search)}`)
      const data: Vocabulary[] = await res.json()
      setSearchResult(data)
    } catch (e) {
      console.error('Failed to search:', e)
    }
  }

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'Enter') {
      handleSearch()
    }
  }

  const handleClear = () => {
    setIsSearching(false)
    setSearch('')
    setPage(1)
  }

  const displayedList = isSearching ? searchResult : (vocabList || [])

  return (
    <div className="page-container">
      <div className="page-search">
        <input
          type="text"
          className="vocab-search-input"
          placeholder="搜索单词或释义..."
          value={search}
          onChange={(e) => setSearch(e.target.value)}
          onKeyDown={handleKeyDown}
        />
        <button className="vocab-search-btn" onClick={handleSearch}>
          搜索
        </button>
        {isSearching && (
          <button className="vocab-search-clear" onClick={handleClear}>
            清除
          </button>
        )}
      </div>

      {isSearching && (
        <div className="page-result-info">
          找到 {searchResult.length} 个结果
        </div>
      )}

      {loading ? (
        <div className="loading">
          <div className="loading-spinner"></div>
          加载中...
        </div>
      ) : displayedList.length === 0 ? (
        <div className="empty-state">
          <div className="empty-state-icon">🔍</div>
          <p className="empty-state-text">没有找到匹配的词汇</p>
        </div>
      ) : (
        <>
          <div className="vocab-grid">
            {displayedList.map((vocab, idx) => (
              <VocabCard
                key={vocab.id || `vocab-${idx}`}
                index={idx}
                word={vocab.word}
                phonetic={vocab.phonetic}
                part_of_speech={vocab.part_of_speech}
                definition={vocab.definition}
              />
            ))}
          </div>

          {!isSearching && totalPages > 1 && (
            <div className="vocab-pagination">
              <button 
                className="vocab-pagination-btn"
                disabled={page <= 1} 
                onClick={() => setPage(p => p - 1)}
              >
                上一页
              </button>
              <span className="vocab-pagination-info">
                {page} / {totalPages}
              </span>
              <button 
                className="vocab-pagination-btn"
                disabled={page >= totalPages} 
                onClick={() => setPage(p => p + 1)}
              >
                下一页
              </button>
            </div>
          )}
        </>
      )}
    </div>
  )
}
