import { useState, useRef } from 'react'
import VocabCard from '../components/VocabCard'
import ModeSlider from '../components/ModeSlider'

interface Vocabulary {
  id: number
  word: string
  phonetic: string | null
  part_of_speech: string | null
  definition: string
  unit: string | null
}

type Mode = 'word' | 'article' | 'mixed'

export default function Random() {
  const [count, setCount] = useState(10)
  const [wordModeWords, setWordModeWords] = useState<Vocabulary[]>([])
  const [loading, setLoading] = useState(false)
  const [capturing, setCapturing] = useState(false)
  const [showChineseWord, setShowChineseWord] = useState(true)
  const [showEnglishWord, setShowEnglishWord] = useState(true)
  const [showChineseArticle, setShowChineseArticle] = useState(true)
  const [showEnglishArticle, setShowEnglishArticle] = useState(true)
  const [article, setArticle] = useState<{english: string, chinese: string} | null>(null)
  const [articleLoading, setArticleLoading] = useState(false)
  const [incrementalMode, setIncrementalMode] = useState(true)
  const [uploading, setUploading] = useState(false)
  const [showSettings, setShowSettings] = useState(false)
  const [showAddWord, setShowAddWord] = useState(false)
  const [newWordEnglish, setNewWordEnglish] = useState('')
  const [newWordChinese, setNewWordChinese] = useState('')
  const [showAdvancedControls, setShowAdvancedControls] = useState(true)
  const [showArticleSection, setShowArticleSection] = useState(true)
  const [showVocabSection, setShowVocabSection] = useState(true)
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
    
    setArticleLoading(true)
    setShowArticleSection(true)
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

  return (
    <div className="page-container">
      {/* 顶部标签栏 - 统一为控制面板切换 */}
      <div className="tab-bar">
        <button 
          className={`tab-item ${showAdvancedControls ? 'active' : ''}`}
          onClick={() => setShowAdvancedControls(!showAdvancedControls)}
          style={{ flex: 1, justifyContent: 'center', fontWeight: 'bold' }}
        >
          {showAdvancedControls ? '▲ 隐藏控制面板' : '▼ 展开控制面板'}
        </button>
      </div>

      <div className="control-bar">
        {showAdvancedControls ? (
          <>
            <div className="control-bar-main">
              <button 
                className="vocab-search-btn" 
                onClick={fetchRandom} 
                disabled={loading || capturing}
              >
                {loading ? '抽取中' : '🎲 随机抽取'}
              </button>
              
              <button 
                className="vocab-search-btn" 
                onClick={() => generateArticle(wordModeWords)} 
                disabled={articleLoading || wordModeWords.length === 0}
                style={{ background: 'var(--accent)' }}
              >
                {articleLoading ? '写作中...' : '✍️ 生成短文'}
              </button>

              <button 
                className="vocab-search-btn" 
                onClick={() => setShowAddWord(true)} 
                style={{ background: 'var(--success)' }}
              >
                ✏️ 录入
              </button>
            </div>

            <div className="control-bar-secondary">
              <label className="action-chip">
                📷 拍照
                <input
                  type="file"
                  accept="image/*"
                  capture="environment"
                  onChange={handleCameraCapture}
                  style={{ display: 'none' }}
                />
              </label>
              <label className="action-chip">
                📤 传图
                <input
                  type="file"
                  accept="image/*"
                  onChange={handleFileUpload}
                  style={{ display: 'none' }}
                />
              </label>
              <button className="action-chip" onClick={() => setShowSettings(true)}>
                ⚙️ 数量({count})
              </button>
              <div 
                className={`action-chip ${incrementalMode ? 'active' : ''}`}
                onClick={() => setIncrementalMode(!incrementalMode)}
              >
                {incrementalMode ? '🔄 增量' : '🔄 覆盖'}
              </div>
              <div 
                className={`action-chip ${showEnglishWord ? 'active' : ''}`}
                onClick={() => setShowEnglishWord(!showEnglishWord)}
              >
                英 {showEnglishWord ? '显' : '隐'}
              </div>
              <div 
                className={`action-chip ${showChineseWord ? 'active' : ''}`}
                onClick={() => setShowChineseWord(!showChineseWord)}
              >
                中 {showChineseWord ? '显' : '隐'}
              </div>
              <button className="action-chip" onClick={() => setWordModeWords([])} style={{ color: 'var(--danger)' }}>
                🗑️ 清空
              </button>
            </div>
          </>
        ) : (
          <div className="page-result-info" style={{ margin: 0, padding: '8px 0', textAlign: 'center' }}>
            {wordModeWords.length > 0 
              ? `已加载 ${wordModeWords.length} 个单词 ${article ? ' | 已生成短文' : ''}` 
              : '列表为空，点击上方展开按钮开始'}
          </div>
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
        <div className="loading" style={{ padding: '40px 0' }}>
          <div className="loading-spinner"></div>
          正在加载单词...
        </div>
      ) : (wordModeWords.length === 0) ? (
        <div className="empty-state">
          <div className="empty-state-icon">🎲</div>
          <p className="empty-state-text">列表为空，请随机抽取或手动录入</p>
        </div>
      ) : (
        <>
          {(article || articleLoading) && (
            <div className="article-section-wrapper" style={{ background: 'white', padding: '20px', borderRadius: '12px', border: '1px solid var(--border-light)', marginBottom: '24px' }}>
              <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: '16px' }}>
                <h3 
                  style={{ margin: 0, fontSize: '18px', display: 'flex', alignItems: 'center', cursor: 'pointer' }}
                  onClick={() => setShowArticleSection(!showArticleSection)}
                >
                  {showArticleSection ? '▼' : '▶'} AI 阅读短文
                </h3>
                <div style={{ display: 'flex', gap: '8px' }}>
                  <button 
                    className={`action-chip ${showEnglishArticle ? 'active' : ''}`}
                    onClick={() => setShowEnglishArticle(!showEnglishArticle)}
                    style={{ fontSize: '12px', padding: '4px 8px' }}
                  >
                    英
                  </button>
                  <button 
                    className={`action-chip ${showChineseArticle ? 'active' : ''}`}
                    onClick={() => setShowChineseArticle(!showChineseArticle)}
                    style={{ fontSize: '12px', padding: '4px 8px' }}
                  >
                    中
                  </button>
                </div>
              </div>

              {showArticleSection && (
                <>
                  {articleLoading && (
                    <div className="loading" style={{ marginBottom: '16px' }}>
                      <div className="loading-spinner"></div>
                      AI 正在努力撰写短文中...
                    </div>
                  )}
                  {article && (
                    <div className="article-content" style={{ display: 'flex', flexDirection: 'column', gap: '20px' }}>
                      {showEnglishArticle && (
                        <div className="article-section">
                          <h3 style={{ marginBottom: '12px', fontSize: '16px', color: 'var(--accent)', borderLeft: '4px solid var(--accent)', paddingLeft: '12px' }}>English Passage</h3>
                          <div style={{ lineHeight: '1.8', fontSize: '16px', textAlign: 'justify', whiteSpace: 'pre-wrap' }}>{article.english}</div>
                        </div>
                      )}
                      {showChineseArticle && (
                        <div className="article-section" style={{ borderTop: '1px solid var(--border-light)', paddingTop: '20px' }}>
                          <h3 style={{ marginBottom: '12px', fontSize: '16px', color: 'var(--accent)', borderLeft: '4px solid var(--accent-light)', paddingLeft: '12px' }}>中文对照</h3>
                          <div style={{ lineHeight: '1.8', fontSize: '16px', color: 'var(--text-secondary)', whiteSpace: 'pre-wrap' }}>{article.chinese}</div>
                        </div>
                      )}
                    </div>
                  )}
                </>
              )}
            </div>
          )}
          
          {wordModeWords.length > 0 && (
            <div className="vocab-section-wrapper">
              <div 
                style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: '16px' }}
              >
                <h3 
                  style={{ margin: 0, fontSize: '18px', cursor: 'pointer' }}
                  onClick={() => setShowVocabSection(!showVocabSection)}
                >
                  {showVocabSection ? '▼' : '▶'} 单词列表 ({wordModeWords.length})
                </h3>
                <div style={{ display: 'flex', gap: '8px' }}>
                  <button 
                    className={`action-chip ${showEnglishWord ? 'active' : ''}`}
                    onClick={() => setShowEnglishWord(!showEnglishWord)}
                    style={{ fontSize: '12px', padding: '4px 8px' }}
                  >
                    英
                  </button>
                  <button 
                    className={`action-chip ${showChineseWord ? 'active' : ''}`}
                    onClick={() => setShowChineseWord(!showChineseWord)}
                    style={{ fontSize: '12px', padding: '4px 8px' }}
                  >
                    中
                  </button>
                </div>
              </div>
              
              {showVocabSection && (
                <div className="vocab-grid">
                  {wordModeWords.map((word, idx) => (
                    <VocabCard
                      key={idx}
                      index={idx}
                      word={word.word}
                      phonetic={word.phonetic}
                      part_of_speech={word.part_of_speech}
                      definition={word.definition}
                      showChinese={showChineseWord}
                      showEnglish={showEnglishWord}
                    />
                  ))}
                </div>
              )}
            </div>
          )}
        </>
      )}

      {/* 悬浮按钮 - 快捷刷新 */}
      {wordModeWords.length > 0 && (
        <button 
          className="fab" 
          onClick={fetchRandom} 
          disabled={loading}
          style={{ bottom: '20px' }}
        >
          {loading ? '...' : '🔄'}
        </button>
      )}
    </div>
  )
}
