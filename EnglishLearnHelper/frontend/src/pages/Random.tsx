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

type Mode = 'word' | 'article'

export default function Random() {
  const [count, setCount] = useState(10)
  const [wordModeWords, setWordModeWords] = useState<Vocabulary[]>([])
  const [articleModeWords, setArticleModeWords] = useState<Vocabulary[]>([])
  const [loading, setLoading] = useState(false)
  const [showChineseWord, setShowChineseWord] = useState(true)
  const [showEnglishWord, setShowEnglishWord] = useState(true)
  const [showChineseArticle, setShowChineseArticle] = useState(true)
  const [showEnglishArticle, setShowEnglishArticle] = useState(true)
  const [mode, setMode] = useState<Mode>('word')
  const [article, setArticle] = useState<{english: string, chinese: string} | null>(null)
  const [articleLoading, setArticleLoading] = useState(false)
  const [showWordsInArticle, setShowWordsInArticle] = useState(true)

  const fetchRandom = async () => {
    setLoading(true)
    setArticle(null)
    try {
      const totalPages = Math.ceil(count / 100)
      let allWords: Vocabulary[] = []
      
      for (let i = 1; i <= totalPages; i++) {
        const res = await fetch(`/api/v1/vocabulary?page=${i}&page_size=100`)
        const data = await res.json()
        allWords = [...allWords, ...data.items]
      }
      
      const shuffled = [...allWords].sort(() => Math.random() - 0.5)
      setWordModeWords(shuffled.slice(0, count))
    } catch (e) {
      console.error('Failed to fetch:', e)
    }
    setLoading(false)
  }

  const generateArticle = async (vocabList: Vocabulary[]) => {
    if (vocabList.length === 0) return
    
    const shuffled = [...vocabList].sort(() => Math.random() - 0.5)
    const wordList = shuffled.map(v => v.word)
    
    setArticleModeWords([...vocabList])
    setArticleLoading(true)
    setShowWordsInArticle(true)
    try {
      const res = await fetch('/api/v1/article', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(wordList)
      })
      const data = await res.json()
      setArticle(data.article)
    } catch (e) {
      console.error('Failed to generate article:', e)
    }
    setArticleLoading(false)
  }

  const handleModeChange = (newMode: Mode) => {
    setMode(newMode)
  }

  return (
    <div className="page-container">
      <div className="page-controls">
        <button 
          className={`mode-btn ${mode === 'word' ? 'active' : ''}`}
          onClick={() => setMode('word')}
        >
          单词模式
        </button>
        <button 
          className={`mode-btn ${mode === 'article' ? 'active' : ''}`}
          onClick={() => setMode('article')}
        >
          短文模式
        </button>
        <input
          type="number"
          className="vocab-search-input"
          value={count}
          onChange={(e) => setCount(Math.max(1, Math.min(100, parseInt(e.target.value) || 1)))}
          min={1}
          max={100}
        />
        <button className="vocab-search-btn" onClick={fetchRandom} disabled={loading}>
          {loading ? '抽取中...' : '🎲 抽取单词'}
        </button>
        <button 
          className="vocab-search-btn" 
          onClick={() => generateArticle(wordModeWords)} 
          disabled={articleLoading || wordModeWords.length === 0}
        >
          {articleLoading ? '生成中...' : '📝 生成短文'}
        </button>
      </div>

      <div className="page-controls">
        {mode === 'word' ? (
          <>
            <label className="checkbox-label">
              <input type="checkbox" checked={showEnglishWord} onChange={(e) => setShowEnglishWord(e.target.checked)} />
              英文
            </label>
            <label className="checkbox-label">
              <input type="checkbox" checked={showChineseWord} onChange={(e) => setShowChineseWord(e.target.checked)} />
              中文
            </label>
          </>
        ) : (
          <>
            <label className="checkbox-label">
              <input type="checkbox" checked={showWordsInArticle} onChange={(e) => setShowWordsInArticle(e.target.checked)} />
              单词
            </label>
            <label className="checkbox-label">
              <input type="checkbox" checked={showEnglishArticle} onChange={(e) => setShowEnglishArticle(e.target.checked)} />
              英文
            </label>
            <label className="checkbox-label">
              <input type="checkbox" checked={showChineseArticle} onChange={(e) => setShowChineseArticle(e.target.checked)} />
              中文
            </label>
          </>
        )}
      </div>

      {loading ? (
        <div className="loading">
          <div className="loading-spinner"></div>
          加载中...
        </div>
      ) : wordModeWords.length === 0 ? (
        <div className="empty-state">
          <div className="empty-state-icon">🎲</div>
          <p className="empty-state-text">请点击"抽取单词"按钮开始</p>
        </div>
      ) : (
        <>
          {mode === 'article' && articleModeWords.length > 0 && showWordsInArticle && (article || articleLoading) && (
            <div className="article-section-wrapper">
              {articleLoading && (
                <div className="loading" style={{ marginBottom: '16px' }}>
                  <div className="loading-spinner"></div>
                  生成短文中...
                </div>
              )}
              {article && (
                <div className="article-content">
                  {showEnglishArticle && (
                    <div className="article-section">
                      <div className="article-label">英文短文</div>
                      <div className="article-text">{article.english}</div>
                    </div>
                  )}
                  {showChineseArticle && (
                    <div className="article-section">
                      <div className="article-label">中文释义</div>
                      <div className="article-text">{article.chinese}</div>
                    </div>
                  )}
                </div>
              )}
            </div>
          )}
          {(mode === 'word' || (articleModeWords.length > 0 && showWordsInArticle)) && (
            <div className="vocab-grid">
              {(mode === 'word' ? wordModeWords : articleModeWords).map((word, idx) => (
                <VocabCard
                  key={idx}
                  index={idx}
                  word={word.word}
                  phonetic={word.phonetic}
                  part_of_speech={word.part_of_speech}
                  definition={word.definition}
                  showChinese={mode === 'word' ? showChineseWord : showChineseArticle}
                  showEnglish={mode === 'word' ? showEnglishWord : showEnglishArticle}
                />
              ))}
            </div>
          )}
        </>
      )}
    </div>
  )
}
