import { useState } from 'react'
import VocabCard from '../components/VocabCard'

interface Vocabulary {
  id: number
  word: string
  phonetic: string | null
  part_of_speech: string | null
  definition: string
  unit: string | null
}

export default function Random() {
  const [count, setCount] = useState(10)
  const [words, setWords] = useState<Vocabulary[]>([])
  const [loading, setLoading] = useState(false)
  const [showChinese, setShowChinese] = useState(true)
  const [showEnglish, setShowEnglish] = useState(true)

  const fetchRandom = async () => {
    setLoading(true)
    try {
      const totalPages = Math.ceil(count / 100)
      let allWords: Vocabulary[] = []
      
      for (let i = 1; i <= totalPages; i++) {
        const res = await fetch(`/api/v1/vocabulary?page=${i}&page_size=100`)
        const data = await res.json()
        allWords = [...allWords, ...data.items]
      }
      
      const shuffled = [...allWords].sort(() => Math.random() - 0.5)
      setWords(shuffled.slice(0, count))
    } catch (e) {
      console.error('Failed to fetch:', e)
    }
    setLoading(false)
  }

  return (
    <div className="page-container">
      <div className="page-controls">
        <button className="mode-btn active">单词模式</button>
        <button className="mode-btn">短文模式</button>
        <label className="checkbox-label">
          <input type="checkbox" checked={showChinese} onChange={(e) => setShowChinese(e.target.checked)} />
          中文
        </label>
        <label className="checkbox-label">
          <input type="checkbox" checked={showEnglish} onChange={(e) => setShowEnglish(e.target.checked)} />
          英文
        </label>
        <input
          type="number"
          className="vocab-search-input"
          value={count}
          onChange={(e) => setCount(Math.max(1, Math.min(100, parseInt(e.target.value) || 1)))}
          min={1}
          max={100}
        />
        <button className="vocab-search-btn" onClick={fetchRandom} disabled={loading}>
          {loading ? '抽取中...' : '🎲 重新抽取'}
        </button>
      </div>

      {loading ? (
        <div className="loading">
          <div className="loading-spinner"></div>
          加载中...
        </div>
      ) : words.length > 0 ? (
        <div className="vocab-grid">
          {words.map((word, idx) => (
            <VocabCard
              key={idx}
              index={idx}
              word={word.word}
              phonetic={word.phonetic}
              part_of_speech={word.part_of_speech}
              definition={word.definition}
              showChinese={showChinese}
              showEnglish={showEnglish}
            />
          ))}
        </div>
      ) : (
        <div className="empty-state">
          <div className="empty-state-icon">🎲</div>
          <p className="empty-state-text">点击上方按钮开始抽取</p>
        </div>
      )}
    </div>
  )
}
