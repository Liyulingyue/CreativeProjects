import { useEffect, useRef, useState } from 'react'
import {
  loadChatHistory,
  saveChatHistory,
  clearChatHistory as clearStorageChat,
  type ChatMessage,
} from '../lib/storage'

interface Props {
  onClose: () => void
  onNewRoute?: (route: any) => void
  currentRoute?: any
  prefs?: any
}

interface ChatMsg {
  role: 'user' | 'agent'
  text: string
  routeChanged?: boolean
}

export function ChatDialog({ onClose, onNewRoute, currentRoute, prefs }: Props) {
  const [messages, setMessages] = useState<ChatMsg[]>(() => {
    const stored = loadChatHistory()
    if (stored.length === 0) {
      return [{ role: 'agent', text: '嗨，我是你的红山导游。走累了？晒了？想换路线？随时告诉我。' }]
    }
    return stored.map((m) => ({
      role: m.role === 'assistant' ? 'agent' : ('user' as const),
      text: m.content,
      routeChanged: !!m.newRoute,
    }))
  })
  const [input, setInput] = useState('')
  const [loading, setLoading] = useState(false)
  const scrollerRef = useRef<HTMLDivElement>(null)

  useEffect(() => {
    scrollerRef.current?.scrollTo({ top: 99999, behavior: 'smooth' })
  }, [messages])

  const QUICK = [
    '走不动了，能少走点吗？',
    '太阳太晒，能换阴凉的路线吗',
    '加上考拉馆',
    '跳过老虎',
    '帮我多看几个馆',
    '想看网红动物',
  ]

  function persistMessages(msgs: ChatMsg[]) {
    const chatMsgs: ChatMessage[] = msgs.map((m) => ({
      role: m.role === 'agent' ? 'assistant' : ('user' as const),
      content: m.text,
      newRoute: m.routeChanged ? {} : undefined,
    }))
    saveChatHistory(chatMsgs)
  }

  async function send(text?: string) {
    const msg = (text ?? input).trim()
    if (!msg || loading) return
    setInput('')
    const newMsgs = [...messages, { role: 'user' as const, text: msg }]
    setMessages(newMsgs)
    setLoading(true)

    const history: ChatMessage[] = newMsgs.map((m) => ({
      role: m.role === 'agent' ? 'assistant' : ('user' as const),
      content: m.text,
    }))

    try {
      const r = await fetch('/api/chat', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          message: msg,
          current_route: currentRoute,
          prefs,
          history,
        }),
      })
      const d = await r.json()
      const reply: ChatMsg = {
        role: 'agent',
        text: d.reply || '…',
        routeChanged: !!d.new_route,
      }
      const updated = [...newMsgs, reply]
      setMessages(updated)
      persistMessages(updated)
      if (d.new_route && onNewRoute) {
        onNewRoute(d.new_route)
      }
    } catch {
      const updated = [...newMsgs, { role: 'agent' as const, text: '网络好像出问题了，试试再说一次？' }]
      setMessages(updated)
      persistMessages(updated)
    } finally {
      setLoading(false)
    }
  }

  return (
    <div className="modal-mask" onClick={onClose}>
      <div
        className="modal"
        onClick={(e) => e.stopPropagation()}
        style={{ maxWidth: 460, height: '80vh', display: 'flex', flexDirection: 'column' }}
      >
        <h3 style={{ margin: '0 0 8px' }}>💬 红山导游（在线）</h3>

        <div
          ref={scrollerRef}
          style={{
            flex: 1,
            overflowY: 'auto',
            background: 'var(--bg)',
            borderRadius: 10,
            padding: 10,
            marginBottom: 10,
            minHeight: 200,
          }}
        >
          {messages.map((m, i) => (
            <div
              key={i}
              style={{
                display: 'flex',
                justifyContent: m.role === 'user' ? 'flex-end' : 'flex-start',
                marginBottom: 8,
              }}
            >
              <div
                style={{
                  maxWidth: '85%',
                  padding: '8px 12px',
                  borderRadius: 10,
                  fontSize: 14,
                  background: m.role === 'user' ? 'var(--primary)' : 'var(--bg-elev)',
                  color: m.role === 'user' ? 'white' : 'var(--fg)',
                  border: m.role === 'agent' ? '1px solid var(--border)' : 'none',
                  lineHeight: 1.5,
                }}
              >
                {m.text}
                {m.routeChanged && (
                  <div
                    style={{
                      marginTop: 6,
                      fontSize: 12,
                      color: 'var(--primary-strong)',
                      fontWeight: 600,
                    }}
                  >
                    ✓ 路线已更新
                  </div>
                )}
              </div>
            </div>
          ))}
          {loading && (
            <div style={{ fontSize: 12, color: 'var(--fg-muted)', padding: '4px 10px' }}>
              思考中…
            </div>
          )}
        </div>

        <div className="quick-feedback" style={{ marginBottom: 8 }}>
          {QUICK.map((q) => (
            <button key={q} onClick={() => send(q)} disabled={loading}>
              {q}
            </button>
          ))}
        </div>

        <div style={{ display: 'flex', gap: 8 }}>
          <input
            type="text"
            value={input}
            onChange={(e) => setInput(e.target.value)}
            onKeyDown={(e) => e.key === 'Enter' && send()}
            placeholder="说点什么…"
            disabled={loading}
            style={{
              flex: 1,
              padding: '10px 14px',
              border: '1px solid var(--border)',
              borderRadius: 10,
              background: '#fff',
              fontSize: 14,
            }}
          />
          <button
            className="btn btn-primary"
            onClick={() => send()}
            disabled={loading || !input.trim()}
            style={{ padding: '10px 16px' }}
          >
            发送
          </button>
        </div>
      </div>
    </div>
  )
}