import { useState, useEffect } from 'react'
import './App.css'

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

function App() {
  const [vocabList, setVocabList] = useState<Vocabulary[]>([])
  const [page, setPage] = useState(1)
  const [totalPages, setTotalPages] = useState(1)
  const [total, setTotal] = useState(0)
  const [search, setSearch] = useState('')
  const [searchResult, setSearchResult] = useState<Vocabulary[]>([])
  const [isSearching, setIsSearching] = useState(false)

  useEffect(() => {
    fetchVocab()
  }, [page])

  const fetchVocab = async () => {
    try {
      const res = await fetch(`http://localhost:8000/api/vocabulary?page=${page}&page_size=50`)
      const data: VocabResponse = await res.json()
      setVocabList(data.items)
      setTotalPages(data.total_pages)
      setTotal(data.total)
    } catch (e) {
      console.error('Failed to fetch vocabulary:', e)
    }
  }

  const handleSearch = async () => {
    if (!search.trim()) {
      setIsSearching(false)
      return
    }
    setIsSearching(true)
    try {
      const res = await fetch(`http://localhost:8000/api/vocabulary/search?q=${encodeURIComponent(search)}`)
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

  const displayedList = isSearching ? searchResult : vocabList

  return (
    <div className="app">
      <header>
        <h1>英语单词学习助手</h1>
        <p>共收录 {total} 个雅思词汇</p>
      </header>

      <div className="search-box">
        <input
          type="text"
          placeholder="搜索单词或释义..."
          value={search}
          onChange={(e) => setSearch(e.target.value)}
          onKeyDown={handleKeyDown}
        />
        <button onClick={handleSearch}>搜索</button>
        {isSearching && (
          <button onClick={() => { setIsSearching(false); setSearch('') }}>返回列表</button>
        )}
      </div>

      <div className="vocab-list">
        {displayedList.map((vocab) => (
          <div key={vocab.id} className="vocab-card">
            <div className="word">{vocab.word}</div>
            {vocab.phonetic && <div className="phonetic">{vocab.phonetic}</div>}
            {vocab.part_of_speech && <div className="pos">{vocab.part_of_speech}</div>}
            <div className="definition">{vocab.definition}</div>
            {vocab.unit && <div className="unit">{vocab.unit}</div>}
          </div>
        ))}
      </div>

      {!isSearching && totalPages > 1 && (
        <div className="pagination">
          <button disabled={page <= 1} onClick={() => setPage(p => p - 1)}>上一页</button>
          <span>{page} / {totalPages}</span>
          <button disabled={page >= totalPages} onClick={() => setPage(p => p + 1)}>下一页</button>
        </div>
      )}
    </div>
  )
}

export default App
