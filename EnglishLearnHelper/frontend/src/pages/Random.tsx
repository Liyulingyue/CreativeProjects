import { useState, useRef } from 'react'
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
  const [capturing, setCapturing] = useState(false)
  const [showChineseWord, setShowChineseWord] = useState(true)
  const [showEnglishWord, setShowEnglishWord] = useState(true)
  const [showChineseArticle, setShowChineseArticle] = useState(true)
  const [showEnglishArticle, setShowEnglishArticle] = useState(true)
  const [mode, setMode] = useState<Mode>('word')
  const [article, setArticle] = useState<{english: string, chinese: string} | null>(null)
  const [articleLoading, setArticleLoading] = useState(false)
  const [showWordsInArticle, setShowWordsInArticle] = useState(true)
  const [incrementalMode, setIncrementalMode] = useState(false)
  const [uploading, setUploading] = useState(false)
  const [showSettings, setShowSettings] = useState(false)
  const [showAddWord, setShowAddWord] = useState(false)
  const [newWordEnglish, setNewWordEnglish] = useState('')
  const [newWordChinese, setNewWordChinese] = useState('')
  const chineseInputRef = useRef<HTMLInputElement>(null)

  const processImageFile = async (file: File) => {
    setUploading(true)
    setLoading(true)
    
    try {
      const formData = new FormData()
      formData.append('file', file)
      
      const ocrRes = await fetch('/api/v1/ocr/vl', {
        method: 'POST',
        body: formData
      })
      const ocrData = await ocrRes.json()
      
      if (ocrData.error) {
        console.error('OCR error:', ocrData.error)
        setLoading(false)
        setUploading(false)
        return
      }
      
      const texts = ocrData.results?.map((r: any) => r.markdown).filter(Boolean) || []
      
      if (texts.length === 0) {
        setLoading(false)
        setUploading(false)
        return
      }
      
      const convertRes = await fetch('/api/v1/ocr/convert', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(texts)
      })
      const convertData = await convertRes.json()
      
      if (convertData.vocabulary) {
        const newWords: Vocabulary[] = convertData.vocabulary.map((v: any, idx: number) => ({
          id: Date.now() + idx,
          word: v.word || '',
          phonetic: v.phonetic || null,
          part_of_speech: v.part_of_speech || null,
          definition: v.definition || '',
          unit: null
        })).filter((w: Vocabulary) => w.word && w.definition)
        
        if (incrementalMode) {
          const existingSet = new Set(wordModeWords.map(w => w.word))
          const uniqueWords = newWords.filter(w => !existingSet.has(w.word))
          setWordModeWords([...wordModeWords, ...uniqueWords])
        } else {
          setWordModeWords(newWords)
        }
      }
    } catch (err) {
      console.error('Upload error:', err)
    }
    
    setLoading(false)
    setUploading(false)
  }

  const handleCameraCapture = async (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0]
    if (!file) return
    
    setCapturing(true)
    await processImageFile(file)
    setCapturing(false)
    e.target.value = ''
  }

  const handleFileUpload = async (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0]
    if (!file) return
    
    await processImageFile(file)
    e.target.value = ''
  }

  const fetchRandom = async () => {
    setLoading(true)
    setArticle(null)
    try {
      const res = await fetch(`/api/v1/vocabulary/random?count=${count}`)
      const data = await res.json()
      const newWords: Vocabulary[] = data.items
      
      if (incrementalMode) {
        const existingWords = wordModeWords
        const existingSet = new Set(existingWords.map(w => w.word))
        const uniqueNewWords = newWords.filter(w => !existingSet.has(w.word))
        setWordModeWords([...existingWords, ...uniqueNewWords])
      } else {
        setWordModeWords(newWords)
      }
    } catch (e) {
      console.error('Failed to fetch:', e)
    }
    setLoading(false)
  }

  const handleAddWord = () => {
    if (!newWordEnglish.trim() || !newWordChinese.trim()) return
    
    const newWord: Vocabulary = {
      id: Date.now(),
      word: newWordEnglish.trim(),
      phonetic: null,
      part_of_speech: null,
      definition: newWordChinese.trim(),
      unit: null
    }
    
    if (incrementalMode) {
      setWordModeWords([...wordModeWords, newWord])
    } else {
      setWordModeWords([newWord])
    }
    
    setNewWordEnglish('')
    setNewWordChinese('')
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
        <div style={{ flex: 1 }} />
        <button 
          className="icon-btn" 
          onClick={() => setShowSettings(true)}
          title="设置"
        >
          ⚙️
        </button>
      </div>

      <div className="page-controls">
        {mode === 'word' ? (
          <>
            <button className="vocab-search-btn" onClick={fetchRandom} disabled={loading || capturing}>
              {loading ? '抽取中...' : '🎲 抽取单词'}
            </button>
            <button className="vocab-search-btn" onClick={() => setShowAddWord(true)}>
              ✏️ 录入单词
            </button>
            <label className="vocab-search-btn" style={{ cursor: capturing ? 'wait' : 'pointer', display: 'inline-flex' }}>
              {capturing ? '识别中...' : '📷 拍照取词'}
              <input
                type="file"
                accept="image/*"
                capture="environment"
                onChange={handleCameraCapture}
                style={{ display: 'none' }}
              />
            </label>
            <label className="vocab-search-btn" style={{ cursor: uploading ? 'wait' : 'pointer', display: 'inline-flex' }}>
              {uploading ? '上传中...' : '📤 传图取词'}
              <input
                type="file"
                accept="image/*"
                onChange={handleFileUpload}
                style={{ display: 'none' }}
              />
            </label>
            <button className="vocab-search-btn" onClick={() => setWordModeWords([])}>
              🗑️ 清空
            </button>
            <label className="checkbox-label">
              <input type="checkbox" checked={incrementalMode} onChange={(e) => setIncrementalMode(e.target.checked)} />
              增量
            </label>
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
            <button 
              className="vocab-search-btn" 
              onClick={() => generateArticle(wordModeWords)} 
              disabled={articleLoading || wordModeWords.length === 0}
            >
              {articleLoading ? '生成中...' : '📝 生成短文'}
            </button>
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

      {showSettings && (
        <div className="modal-overlay" onClick={() => setShowSettings(false)}>
          <div className="modal" onClick={e => e.stopPropagation()}>
            <h3>设置</h3>
            <div className="modal-content">
              <label>
                抽取数量：
                <input
                  type="number"
                  value={count}
                  onChange={(e) => setCount(Math.max(1, Math.min(100, parseInt(e.target.value) || 1)))}
                  min={1}
                  max={100}
                  className="vocab-search-input"
                />
              </label>
            </div>
            <div className="modal-actions">
              <button className="vocab-search-btn" onClick={() => setShowSettings(false)}>
                确认
              </button>
            </div>
          </div>
        </div>
      )}

      {showAddWord && (
        <div className="modal-overlay" onClick={() => setShowAddWord(false)}>
          <div className="modal" onClick={e => e.stopPropagation()}>
            <h3>录入单词</h3>
            <div className="modal-content">
              <label className="block">
                英文：
                <input
                  type="text"
                  value={newWordEnglish}
                  onChange={(e) => setNewWordEnglish(e.target.value)}
                  className="vocab-search-input"
                  placeholder="请输入英文单词"
                  autoFocus
                  onKeyDown={(e) => {
                    if (e.key === 'Enter') {
                      e.preventDefault()
                      chineseInputRef.current?.focus()
                    }
                  }}
                />
              </label>
              <label className="block" style={{ marginTop: '12px' }}>
                中文：
                <input
                  ref={chineseInputRef}
                  type="text"
                  value={newWordChinese}
                  onChange={(e) => setNewWordChinese(e.target.value)}
                  className="vocab-search-input"
                  placeholder="请输入中文含义"
                  onKeyDown={(e) => {
                    if (e.key === 'Enter') {
                      e.preventDefault()
                      if (newWordEnglish.trim() && newWordChinese.trim()) {
                        handleAddWord()
                      }
                    }
                  }}
                />
              </label>
            </div>
            <div className="modal-actions">
              <button 
                className="vocab-search-btn" 
                onClick={handleAddWord}
                disabled={!newWordEnglish.trim() || !newWordChinese.trim()}
                style={{ minWidth: '80px' }}
              >
                提交
              </button>
              <button 
                className="vocab-search-clear" 
                onClick={() => setShowAddWord(false)}
                style={{ minWidth: '80px', marginLeft: '8px' }}
              >
                关闭
              </button>
            </div>
          </div>
        </div>
      )}

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
