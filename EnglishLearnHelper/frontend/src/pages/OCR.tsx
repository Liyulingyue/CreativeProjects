import { useState } from 'react'
import VocabCard from '../components/VocabCard'

interface OCRResult {
  texts: string[]
  image_url: string
}

interface VocabItem {
  word: string
  phonetic: string
  part_of_speech: string
  definition: string
}

export default function OCR() {
  const [file, setFile] = useState<File | null>(null)
  const [results, setResults] = useState<OCRResult[]>([])
  const [loading, setLoading] = useState(false)
  const [converting, setConverting] = useState(false)
  const [vocabulary, setVocabulary] = useState<VocabItem[]>([])
  const [error, setError] = useState('')

  const handleFileChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    if (e.target.files && e.target.files[0]) {
      setFile(e.target.files[0])
      setResults([])
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
    
    try {
      const res = await fetch('/api/v1/ocr/image', {
        method: 'POST',
        body: formData
      })
      const data = await res.json()
      
      if (data.error) {
        setError(data.error)
      } else if (data.results) {
        setResults(data.results)
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
    if (results.length === 0) return
    
    setConverting(true)
    setError('')
    
    const allTexts = results.flatMap(r => r.texts)
    
    try {
      const res = await fetch('/api/v1/ocr/convert', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(allTexts)
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

  return (
    <div className="page-container">
      <div className="page-controls">
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
        {results.length > 0 && (
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

      {results.length > 0 && vocabulary.length === 0 && (
        <div className="ocr-results">
          <h3>OCR 识别结果</h3>
          {results.map((result, idx) => (
            <div key={idx} className="ocr-item">
              {result.texts.map((text, i) => (
                <div key={i} className="ocr-text">{text}</div>
              ))}
            </div>
          ))}
        </div>
      )}
    </div>
  )
}
