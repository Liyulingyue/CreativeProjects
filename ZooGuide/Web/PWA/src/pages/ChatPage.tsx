import { useEffect, useRef, useState } from 'react'
import type { Route, UserPreference } from '../types'

interface Props {
  currentRoute: Route | null
  prefs: UserPreference | null
  onRouteUpdate: (r: Route) => void
  onGoPlan: () => void
  onGoActivity: () => void
}

interface ChatMsg {
  id: number
  role: 'user' | 'agent'
  text: string
  constraint?: any
  questions?: string[]
  route?: any
}

const QUICK = [
  '走不动了',
  '太晒了',
  '加考拉馆',
  '跳老虎',
  '想多逛',
  '看网红',
]

const INITIAL_MSG: ChatMsg = {
  id: 0,
  role: 'agent',
  text: '嗨，我是你的红山导游 🦒。想逛哪些馆？走累了？想看什么动物？随时告诉我。',
}

export function ChatPage({ currentRoute, prefs, onRouteUpdate, onGoPlan, onGoActivity }: Props) {
  const [messages, setMessages] = useState<ChatMsg[]>([INITIAL_MSG])
  const [input, setInput] = useState('')
  const [loading, setLoading] = useState(false)
  const [showQuick, setShowQuick] = useState(true)
  const scrollerRef = useRef<HTMLDivElement>(null)
  const idRef = useRef(1)

  useEffect(() => {
    setTimeout(() => {
      scrollerRef.current?.scrollTo({ top: 999999, behavior: 'smooth' })
    }, 50)
  }, [messages, loading])

  async function send(text?: string) {
    const msg = (text ?? input).trim()
    if (!msg || loading) return
    setInput('')
    const userMsg: ChatMsg = { id: idRef.current++, role: 'user', text: msg }
    setMessages((prev) => [...prev, userMsg])
    setLoading(true)
    try {
      const r = await fetch('/api/chat', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          message: msg,
          current_route: currentRoute,
          prefs,
          history: messages.slice(-6).map((m) => ({
            role: m.role === 'agent' ? 'assistant' : 'user',
            content: m.text,
          })),
        }),
      })
      const d = await r.json()
      const reply: ChatMsg = {
        id: idRef.current++,
        role: 'agent',
        text: d.reply || '…',
        constraint: d.extracted_constraint,
        questions: d.questions || [],
        route: d.new_route,
      }
      setMessages((prev) => [...prev, reply])
      if (d.new_route && onRouteUpdate) {
        onRouteUpdate(d.new_route)
      }
    } catch (e) {
      setMessages((prev) => [
        ...prev,
        { id: idRef.current++, role: 'agent', text: '网络好像出问题了，试试再说一次？' },
      ])
    } finally {
      setLoading(false)
    }
  }

  function reset() {
    setMessages([INITIAL_MSG])
    idRef.current = 1
  }

  return (
    <div className="chat-page">
      {/* Context banner */}
      <div className="chat-context">
        {currentRoute ? (
          <>
            <div className="chat-context-info">
              <div className="chat-context-label">当前路线</div>
              <div className="chat-context-meta">
                {currentRoute.stops.length} 馆 ·{' '}
                {Math.round(currentRoute.total_minutes / 60 * 10) / 10}h
              </div>
            </div>
            <button className="pill-btn primary" onClick={onGoPlan}>
              打开路线
            </button>
          </>
        ) : (
          <>
            <div className="chat-context-info">
              <div className="chat-context-label">💡 还没有路线</div>
            </div>
            <button className="pill-btn primary" onClick={onGoPlan}>
              去规划
            </button>
          </>
        )}
      </div>

      {/* Messages (scrollable) */}
      <div ref={scrollerRef} className="chat-messages">
        {messages.map((m) => (
          <MessageBubble key={m.id} msg={m} />
        ))}
        {loading && <TypingBubble />}
        {!currentRoute && messages.length <= 1 && (
          <button
            className="chat-suggest-btn"
            onClick={onGoActivity}
          >
            📍 没规划？先去拍张照/逛逛 →
          </button>
        )}
      </div>

      {/* Quick replies (collapsible) */}
      {showQuick && (
        <div className="chat-quick-row">
          {QUICK.map((q) => (
            <button key={q} className="chat-quick-chip" onClick={() => send(q)} disabled={loading}>
              {q}
            </button>
          ))}
          <button
            className="chat-quick-toggle"
            onClick={() => setShowQuick(false)}
            title="收起"
          >
            −
          </button>
        </div>
      )}
      {!showQuick && (
        <button className="chat-quick-show" onClick={() => setShowQuick(true)}>
          快捷回复 ▾
        </button>
      )}

      {/* Input (sticky bottom) */}
      <div className="chat-composer">
        <input
          type="text"
          value={input}
          onChange={(e) => setInput(e.target.value)}
          onKeyDown={(e) => e.key === 'Enter' && send()}
          placeholder="说点什么…"
          disabled={loading}
        />
        <button
          className="chat-send-btn"
          onClick={() => send()}
          disabled={loading || !input.trim()}
        >
          {loading ? '⏳' : '➤'}
        </button>
      </div>

      {messages.length > 2 && (
        <button className="chat-clear-link" onClick={reset}>
          清空对话
        </button>
      )}
    </div>
  )
}

function MessageBubble({ msg }: { msg: ChatMsg }) {
  const isUser = msg.role === 'user'
  return (
    <div
      style={{
        display: 'flex',
        justifyContent: isUser ? 'flex-end' : 'flex-start',
        marginBottom: 12,
      }}
    >
      <div
        style={{
          maxWidth: '78%',
          padding: '10px 14px',
          borderRadius: isUser ? '18px 18px 4px 18px' : '18px 18px 18px 4px',
          fontSize: 15,
          lineHeight: 1.5,
          background: isUser ? 'var(--primary)' : 'var(--bg-elev)',
          color: isUser ? 'white' : 'var(--fg)',
          border: isUser ? 'none' : '1px solid var(--border)',
          boxShadow: isUser ? '0 1px 4px rgba(45,106,79,0.25)' : '0 1px 2px rgba(0,0,0,0.04)',
        }}
      >
        {msg.text}
        {msg.constraint && (
          <div style={{ marginTop: 6, fontSize: 11, opacity: 0.85 }}>
            💡 {msg.constraint.type}
            {msg.constraint.venue_name && ` → ${msg.constraint.venue_name}`}
          </div>
        )}
        {msg.questions && msg.questions.length > 0 && (
          <div
            style={{
              marginTop: 6,
              fontSize: 12,
              background: 'var(--primary-soft)',
              padding: '6px 10px',
              borderRadius: 8,
              color: 'var(--primary-strong)',
            }}
          >
            🤔 {msg.questions[0]}
          </div>
        )}
      </div>
    </div>
  )
}

function TypingBubble() {
  return (
    <div style={{ display: 'flex', justifyContent: 'flex-start', marginBottom: 12 }}>
      <div
        style={{
          padding: '12px 16px',
          borderRadius: '18px 18px 18px 4px',
          background: 'var(--bg-elev)',
          border: '1px solid var(--border)',
          display: 'flex',
          gap: 4,
        }}
      >
        <span className="typing-dot" />
        <span className="typing-dot" style={{ animationDelay: '0.2s' }} />
        <span className="typing-dot" style={{ animationDelay: '0.4s' }} />
      </div>
    </div>
  )
}