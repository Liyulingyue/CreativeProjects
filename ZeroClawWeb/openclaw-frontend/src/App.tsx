import { useEffect, useRef, useState } from 'react'
import ReactMarkdown from 'react-markdown'
import remarkGfm from 'remark-gfm'
import { Dexie, type EntityTable } from 'dexie'
import { useLiveQuery } from 'dexie-react-hooks'
import './App.css'

// --- Database Configuration ---
interface ChatRecord {
  id?: number
  baseUrl: string
  token?: string
  user: string
  assistant: string
  ts: number
}

const db = new Dexie('ZeroClawDB') as Dexie & {
  history: EntityTable<ChatRecord, 'id'>
}

db.version(1).stores({
  history: '++id, ts, baseUrl' // primary key 'id' auto-incremented
})

// --- Types ---
type Message = {
  id: string
  role: 'user' | 'assistant'
  text: string
  ts: number
  status?: 'loading' | 'error' | 'done'
}

function genId(prefix = '') {
  return prefix + Date.now().toString(36) + Math.random().toString(36).slice(2, 8)
}

export default function App() {
  // Config state (kept in localStorage for convenience)
  const [baseUrl, setBaseUrl] = useState<string>(() => localStorage.getItem('zc_url') || 'http://127.0.0.1:3000')
  const [token, setToken] = useState<string>(() => localStorage.getItem('zc_token') || '')
  
  const [health, setHealth] = useState<'unknown' | 'healthy' | 'unhealthy' | 'error'>('unknown')
  const [autoHealth, setAutoHealth] = useState<boolean>(true)

  const [input, setInput] = useState<string>('')
  const [messages, setMessages] = useState<Message[]>([])
  const [loading, setLoading] = useState(false)

  // Reactive history from IndexedDB using Dexie
  const history = useLiveQuery(() => db.history.orderBy('ts').reverse().toArray())
  const chatRef = useRef<HTMLDivElement | null>(null)

  // Persist config items
  useEffect(() => {
    localStorage.setItem('zc_url', baseUrl)
    localStorage.setItem('zc_token', token)
  }, [baseUrl, token])

  useEffect(() => {
    let t: number | undefined
    if (autoHealth) {
      checkHealth()
      t = window.setInterval(() => checkHealth(), 10_000)
    }
    return () => {
      if (t) clearInterval(t)
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [baseUrl, token, autoHealth])

  useEffect(() => {
    const el = chatRef.current
    if (el) el.scrollTop = el.scrollHeight
  }, [messages])

  async function checkHealth() {
    const url = baseUrl.replace(/\/$/, '') + '/health'
    try {
      const res = await fetch(url, { method: 'GET' })
      setHealth(res.ok ? 'healthy' : 'unhealthy')
    } catch (err) {
      setHealth('error')
    }
  }

  function loadConversationFromHistory(h: ChatRecord) {
    setBaseUrl(h.baseUrl)
    setToken(h.token || '')
    setMessages([
      { id: genId('u_'), role: 'user', text: h.user, ts: h.ts, status: 'done' },
      { id: genId('a_'), role: 'assistant', text: h.assistant, ts: h.ts, status: 'done' },
    ])
  }

  async function clearHistory() {
    if (!confirm('确认清空本地数据库中的所有历史记录？')) return
    await db.history.clear()
  }

  function exportHistory() {
    if (!history) return
    const blob = new Blob([JSON.stringify(history, null, 2)], { type: 'application/json' })
    const url = URL.createObjectURL(blob)
    const a = document.createElement('a')
    a.href = url
    a.download = 'zeroclaw-db-export.json'
    a.click()
    URL.revokeObjectURL(url)
  }

  async function sendMessage() {
    const prompt = input.trim()
    if (!prompt) return
    
    const userMsg: Message = { id: genId('u_'), role: 'user', text: prompt, ts: Date.now(), status: 'done' }
    const assistantMsg: Message = { id: genId('a_'), role: 'assistant', text: '…', ts: Date.now(), status: 'loading' }
    
    setMessages([userMsg, assistantMsg]) // Single turn view
    setInput('')
    setLoading(true)

    const url = baseUrl.replace(/\/$/, '') + '/webhook'
    try {
      const headers: Record<string, string> = { 'Content-Type': 'application/json' }
      if (token) headers['X-Pairing-Code'] = token
      
      const res = await fetch(url, { method: 'POST', headers, body: JSON.stringify({ message: prompt }) })

      let assistantText = ''
      if (res.ok) {
        const ct = res.headers.get('content-type') || ''
        if (ct.includes('application/json')) {
          const data = await res.json()
          if (data.response) assistantText = String(data.response)
          else if (data.message) assistantText = String(data.message)
          else if (data.text) assistantText = String(data.text)
          else if (data.output) assistantText = typeof data.output === 'string' ? data.output : JSON.stringify(data.output, null, 2)
          else assistantText = JSON.stringify(data, null, 2)
        } else {
          assistantText = await res.text()
        }
      } else {
        assistantText = `请求失败 (HTTP ${res.status}): ${res.statusText}`
      }

      setMessages((s) => s.map((m) => (m.id === assistantMsg.id ? { ...m, text: assistantText, status: 'done' } : m)))

      // --- SAVE TO INDEXEDDB ---
      await db.history.add({
        baseUrl,
        token: token || undefined,
        user: prompt,
        assistant: assistantText,
        ts: Date.now()
      })

    } catch (err: any) {
      const errMsg = String(err.message || err)
      setMessages((s) => s.map((m) => (m.id === assistantMsg.id ? { ...m, text: `网络错误: ${errMsg}`, status: 'error' } : m)))
    } finally {
      setLoading(false)
    }
  }

  return (
    <div id="root">
      <header className="topbar">
        <h1><span>🧠</span> ZeroClaw Web</h1>
        <div className="config">
          <input className="input-url" placeholder="Gateway URL" value={baseUrl} onChange={(e) => setBaseUrl(e.target.value)} />
          <input className="input-token" placeholder="X-Pairing-Code (可选)" value={token} onChange={(e) => setToken(e.target.value)} />
          <button onClick={() => checkHealth()} className="btn">检查</button>
          <label className="auto-check muted">
            <input type="checkbox" checked={autoHealth} onChange={(e) => setAutoHealth(e.target.checked)} /> 自动检测
          </label>
          <span className={`status ${health}`}>
            {health === 'healthy' ? '● 在线' : health === 'unknown' ? '○ 未知' : '● 离线'}
          </span>
        </div>
      </header>

      <main className="chat-layout">
        <aside className="sidebar">
          <div className="sidebar-head">
            <strong>历史记录</strong>
            <div style={{ display: 'flex', gap: '4px' }}>
              <button className="btn small" onClick={exportHistory} title="导出 JSON">导出</button>
              <button className="btn small danger" onClick={clearHistory}>清空</button>
            </div>
          </div>
          <div className="history-list">
            {history.length === 0 && <div className="muted" style={{ padding: '20px', textAlign: 'center', fontSize: '0.85rem' }}>暂无会话</div>}
            {history && history.map((h) => (
              <div key={h.id} className="history-item">
                <div className="history-main" onClick={() => loadConversationFromHistory(h)}>
                  <div className="history-title">{h.user}</div>
                  <div className="history-sub">{new Date(h.ts).toLocaleString()}</div>
                </div>
                <button className="btn tiny danger" onClick={async (e) => { e.stopPropagation(); if (h.id) await db.history.delete(h.id) }}>×</button>
              </div>
            ))}
          </div>
        </aside>

        <section className="chat-section">
          <div className="chat-window" ref={chatRef}>
            {messages.length === 0 && (
              <div className="muted" style={{ flex: 1, display: 'flex', alignItems: 'center', justifyContent: 'center', flexDirection: 'column', gap: '1rem' }}>
                <div style={{ fontSize: '3rem' }}>👋</div>
                <p>准备好开始对话了吗？</p>
              </div>
            )}
            {messages.map((m) => (
              <div key={m.id} className={`message ${m.role}`}>
                <div className="bubble">
                  <div className="text">
                    <ReactMarkdown remarkPlugins={[remarkGfm]}>
                      {m.text}
                    </ReactMarkdown>
                  </div>
                  <div className="meta">{m.status === 'loading' ? '正在请求…' : new Date(m.ts).toLocaleTimeString()}</div>
                </div>
              </div>
            ))}
          </div>

          <div className="composer-container">
            <div className="composer">
              <textarea
                value={input}
                onChange={(e) => setInput(e.target.value)}
                placeholder="发送消息..."
                onKeyDown={(e) => {
                  if (e.key === 'Enter' && !e.shiftKey) {
                    e.preventDefault()
                    sendMessage()
                  }
                }}
              />
              <div className="composer-actions">
                <div className="muted" style={{ fontSize: '0.75rem' }}>Shift + Enter 换行</div>
                <div style={{ display: 'flex', gap: '8px' }}>
                  <button className="btn" onClick={() => { setInput(''); setMessages([]) }}>重置</button>
                  <button className="btn primary" onClick={sendMessage} disabled={loading || !input.trim()}>
                    {loading ? '发送中...' : '发送'}
                  </button>
                </div>
              </div>
            </div>
          </div>
        </section>
      </main>

      <footer className="footer">
        单轮对话 · <code>POST /webhook</code> · <code>GET /health</code>
      </footer>
    </div>
  )
}
