import { useState } from 'react'
import VocabCard from '../components/VocabCard'

interface OCRResult {
  texts: string[]
  image_url: string
}

interface LayoutResult {
  markdown: string
  type: string
}

interface VocabItem {
  word: string
  phonetic: string
  part_of_speech: string
  definition: string
}

type OCRMode = 'v5' | 'vl' | 'structure'

export default function OCR() {
  const [file, setFile] = useState<File | null>(null)
  const [ocrV5Results, setOcrV5Results] = useState<OCRResult[]>([])
  const [layoutResults, setLayoutResults] = useState<LayoutResult[]>([])
  const [loading, setLoading] = useState(false)
  const [converting, setConverting] = useState(false)
  const [vocabulary, setVocabulary] = useState<VocabItem[]>([])
  const [error, setError] = useState('')
  const [mode, setMode] = useState<OCRMode>('v5')

  const handleFileChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    if (e.target.files && e.target.files[0]) {
      setFile(e.target.files[0])
      setOcrV5Results([])
      setLayoutResults([])
      setVocabulary([])
      setError('')
    }
  }

  const handleUpload = async () => {
    if (!file) return
    
    setLoading(true)
    setError('')
    
    const formData = new FormData()
    formData.append('file', file)
    
    const endpoints: Record<OCRMode, string> = {
      'v5': '/api/v1/ocr/v5',
      'vl': '/api/v1/ocr/vl',
      'structure': '/api/v1/ocr/structure'
    }
    
    try {
      const res = await fetch(endpoints[mode], {
        method: 'POST',
        body: formData
      })
      const data = await res.json()
      
      if (data.error) {
        setError(data.error)
      } else if (data.results) {
        if (mode === 'v5') {
          setOcrV5Results(data.results)
        } else {
          setLayoutResults(data.results)
        }
      } else {
        setError('未知响应格式')
      }
    } catch (e) {
      setError('上传失败')
      console.error(e)
    }
    
    setLoading(false)
  }

  const handleConvert = async () => {
    let texts: string[] = []
    
    if (mode === 'v5') {
      texts = ocrV5Results.flatMap(r => r.texts)
    } else {
      texts = layoutResults.map(r => r.markdown)
    }
    
    if (texts.length === 0) return
    
    setConverting(true)
    setError('')
    
    try {
      const res = await fetch('/api/v1/ocr/convert', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(texts)
      })
      const data = await res.json()
      
      if (data.error) {
        setError(data.error)
      } else if (data.vocabulary) {
        setVocabulary(data.vocabulary)
      } else {
        setError('转换失败')
      }
    } catch (e) {
      setError('转换失败')
      console.error(e)
    }
    
    setConverting(false)
  }

  const hasResults = mode === 'v5' ? ocrV5Results.length > 0 : layoutResults.length > 0

  return (
    <div className="page-container">
      <div className="page-controls">
        <div className="mode-selector">
          <button 
            className={`mode-btn ${mode === 'v5' ? 'active' : ''}`}
            onClick={() => setMode('v5')}
          >
            OCRv5
          </button>
          <button 
            className={`mode-btn ${mode === 'vl' ? 'active' : ''}`}
            onClick={() => setMode('vl')}
          >
            OCRVL
          </button>
          <button 
            className={`mode-btn ${mode === 'structure' ? 'active' : ''}`}
            onClick={() => setMode('structure')}
          >
            PPStructureV3
          </button>
        </div>
        <input
          type="file"
          accept="image/*"
          onChange={handleFileChange}
          className="file-input"
        />
        <button 
          className="vocab-search-btn" 
          onClick={handleUpload}
          disabled={!file || loading}
        >
          {loading ? '识别中...' : '开始识别'}
        </button>
        {hasResults && (
          <button 
            className="vocab-search-btn" 
            onClick={handleConvert}
            disabled={converting}
          >
            {converting ? '转换中...' : '转换为词汇'}
          </button>
        )}
      </div>

      {error && (
        <div className="error-message">{error}</div>
      )}

      {vocabulary.length > 0 && (
        <div className="vocab-grid">
          {vocabulary.map((item, idx) => (
            <VocabCard
              key={idx}
              index={idx}
              word={item.word}
              phonetic={item.phonetic || null}
              part_of_speech={item.part_of_speech || null}
              definition={item.definition}
              showChinese={true}
              showEnglish={true}
            />
          ))}
        </div>
      )}

      {hasResults && vocabulary.length === 0 && mode === 'v5' && (
        <div className="ocr-results">
          <h3>OCRv5 识别结果</h3>
          {ocrV5Results.map((result, idx) => (
            <div key={idx} className="ocr-item">
              {result.texts.map((text, i) => (
                <div key={i} className="ocr-text">{text}</div>
              ))}
            </div>
          ))}
        </div>
      )}

      {hasResults && vocabulary.length === 0 && mode !== 'v5' && (
        <div className="ocr-results">
          <h3>{mode === 'vl' ? 'OCRVL' : 'PPStructureV3'} 识别结果</h3>
          {layoutResults.map((result, idx) => (
            <div key={idx} className="ocr-item">
              <div className="ocr-type">类型: {result.type}</div>
              <pre className="ocr-text">{result.markdown}</pre>
            </div>
          ))}
        </div>
      )}
    </div>
  )
}
