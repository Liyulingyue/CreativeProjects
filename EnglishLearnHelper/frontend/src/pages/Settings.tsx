import { useState, useEffect } from 'react'

interface Config {
  MODEL_URL: string
  MODEL_NAME: string
  MODEL_KEY: string
  BAIDU_AISTUDIO_KEY: string
}

export default function Settings() {
  const [config, setConfig] = useState<Config>({
    MODEL_URL: '',
    MODEL_NAME: '',
    MODEL_KEY: '',
    BAIDU_AISTUDIO_KEY: ''
  })
  const [url, setUrl] = useState('')
  const [key, setKey] = useState('')
  const [model, setModel] = useState('')
  const [baiduKey, setBaiduKey] = useState('')
  const [saving, setSaving] = useState(false)
  const [message, setMessage] = useState('')

  useEffect(() => {
    fetchConfig()
  }, [])

  const fetchConfig = async () => {
    try {
      const res = await fetch('/api/v1/config')
      const data = await res.json()
      setConfig(data)
      setUrl(data.MODEL_URL || '')
      setModel(data.MODEL_NAME || '')
      setBaiduKey(data.BAIDU_AISTUDIO_KEY || '')
    } catch (e) {
      console.error('Failed to fetch config:', e)
    }
  }

  const handleSave = async () => {
    setSaving(true)
    setMessage('')
    try {
      const res = await fetch('/api/v1/config', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ url, key, model, baidu_key: baiduKey })
      })
      const data = await res.json()
      setConfig(data)
      setKey('')
      setMessage('保存成功')
      setTimeout(() => setMessage(''), 3000)
    } catch (e) {
      console.error('Failed to save config:', e)
      setMessage('保存失败')
    }
    setSaving(false)
  }

  return (
    <div className="page-container">
      <div className="settings-card">
        <h2 className="settings-title">模型配置</h2>
        
        <div className="settings-item">
          <label className="settings-label">API URL</label>
          <input
            type="text"
            className="settings-input"
            value={url}
            onChange={(e) => setUrl(e.target.value)}
            placeholder="https://api.openai.com/v1"
          />
          <div className="settings-preview">
            当前值: <span className="settings-value">{config.MODEL_URL}</span>
          </div>
        </div>

        <div className="settings-item">
          <label className="settings-label">API Key</label>
          <input
            type="password"
            className="settings-input"
            value={key}
            onChange={(e) => setKey(e.target.value)}
            placeholder="请输入新的 API Key"
          />
          <div className="settings-preview">
            当前值: <span className="settings-value"></span>
          </div>
        </div>

        <div className="settings-item">
          <label className="settings-label">Model</label>
          <input
            type="text"
            className="settings-input"
            value={model}
            onChange={(e) => setModel(e.target.value)}
            placeholder="gpt-4"
          />
          <div className="settings-preview">
            当前值: <span className="settings-value">{config.MODEL_NAME}</span>
          </div>
        </div>

        <div className="settings-item">
          <label className="settings-label">Baidu AI Studio Key</label>
          <input
            type="password"
            className="settings-input"
            value={baiduKey}
            onChange={(e) => setBaiduKey(e.target.value)}
            placeholder="请输入新的 Baidu AI Studio Key"
          />
          <div className="settings-preview">
            当前值: <span className="settings-value"></span>
          </div>
        </div>

        <div className="settings-actions">
          <button
            className="settings-save-btn"
            onClick={handleSave}
            disabled={saving}
          >
            {saving ? '保存中...' : '保存配置'}
          </button>
          {message && (
            <span className={`settings-message ${message.includes('成功') ? 'success' : 'error'}`}>
              {message}
            </span>
          )}
        </div>
      </div>
    </div>
  )
}
