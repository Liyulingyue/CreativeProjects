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
  role: 'user' | 'agent'
  text: string
  constraint?: any
  questions?: string[]
  route?: any
}

const QUICK = [
  '走不动了，能少走点吗？',
  '太阳太晒，换阴凉的路线',
  '加上考拉馆',
  '跳过老虎',
  '帮我多看几个馆',
  '想看网红动物',
]

const INITIAL_MSG: ChatMsg = {
  role: 'agent',
  text: '嗨，我是你的红山导游 🦒。想逛哪些馆？走累了？想看什么动物？随时告诉我。',
}

export function ChatPage({ currentRoute, prefs, onRouteUpdate, onGoPlan, onGoActivity }: Props) {
  const [messages, setMessages] = useState<ChatMsg[]>([INITIAL_MSG])
  const [input, setInput] = useState('')
  const [loading, setLoading] = useState(false)
  const scrollerRef = useRef<HTMLDivElement>(null)

  useEffect(() => {
    scrollerRef.current?.scrollTo({ top: 99999, behavior: 'smooth' })
  }, [messages, loading])

  async function send(text?: string) {
    const msg = (text ?? input).trim()
    if (!msg || loading) return
    setInput('')
    const newMsgs = [...messages, { role: 'user' as const, text: msg }]
    setMessages(newMsgs)
    setLoading(true)
    try {
      const r = await fetch('/api/chat', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          message: msg,
          current_route: currentRoute,
          prefs,
          history: newMsgs.slice(-6).map((m) => ({
            role: m.role === 'agent' ? 'assistant' : 'user',
            content: m.text,
          })),
        }),
      })
      const d = await r.json()
      const reply: ChatMsg = {
        role: 'agent',
        text: d.reply || '…',
        constraint: d.extracted_constraint,
        questions: d.questions || [],
        route: d.new_route,
      }
      setMessages((prev) => [...prev, reply])
      if (d.new_route) {
        onRouteUpdate(d.new_route)
      }
    } catch (e) {
      setMessages((prev) => [
        ...prev,
        { role: 'agent', text: '网络好像出问题了，试试再说一次？' },
      ])
    } finally {
      setLoading(false)
    }
  }

  function reset() {
    setMessages([INITIAL_MSG])
  }

  return (
    <div className="chat-page">
      {/* Context banner - prominent pill button */}
      <div className="chat-context">
        {currentRoute ? (
          <>
            <div style={{ display: 'flex', alignItems: 'center', gap: 8, flex: 1 }}>
              <span style={{ fontSize: 22 }}>🧭</span>
              <div>
                <div style={{ fontSize: 11, color: 'var(--fg-muted)' }}>当前路线</div>
                <div
                  style={{
                    fontSize: 14,
                    fontWeight: 700,
                    color: 'var(--primary-strong)',
                    lineHeight: 1.2,
                  }}
                >
                  {currentRoute.stops.length} 馆 ·{' '}
                  {Math.round(currentRoute.total_minutes / 60 * 10) / 10}h
                </div>
              </div>
            </div>
            <button className="pill-btn primary" onClick={onGoPlan}>
              打开完整路线
              <span style={{ marginLeft: 4 }}>→</span>
            </button>
          </>
        ) : (
          <>
            <div style={{ display: 'flex', alignItems: 'center', gap: 8, flex: 1 }}>
              <span style={{ fontSize: 22 }}>💡</span>
              <div style={{ fontSize: 13, color: 'var(--fg-muted)', lineHeight: 1.4 }}>
                你还没有规划路线
              </div>
            </div>
            <button className="pill-btn primary" onClick={onGoPlan}>
              去规划
              <span style={{ marginLeft: 4 }}>→</span>
            </button>
          </>
        )}
      </div>

      {/* Messages */}
      <div ref={scrollerRef} className="chat-messages">
        {messages.map((m, i) => (
          <div
            key={i}
            style={{
              display: 'flex',
              justifyContent: m.role === 'user' ? 'flex-end' : 'flex-start',
              marginBottom: 10,
            }}
          >
            <div
              style={{
                maxWidth: '85%',
                padding: '10px 14px',
                borderRadius: 12,
                fontSize: 14,
                background: m.role === 'user' ? 'var(--primary)' : 'var(--bg-elev)',
                color: m.role === 'user' ? 'white' : 'var(--fg)',
                border: m.role === 'agent' ? '1px solid var(--border)' : 'none',
                lineHeight: 1.55,
              }}
            >
              {m.text}
              {m.constraint && (
                <div style={{ marginTop: 6, fontSize: 11, opacity: 0.85 }}>
                  💡 {m.constraint.type}
                  {m.constraint.venue_name && ` → ${m.constraint.venue_name}`}
                </div>
              )}
              {m.questions && m.questions.length > 0 && (
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
                  🤔 {m.questions[0]}
                </div>
              )}
              {m.route && (
                <div
                  style={{
                    marginTop: 8,
                    padding: '10px 12px',
                    background: 'var(--bg)',
                    borderRadius: 10,
                    border: '1px solid var(--primary)',
                  }}
                >
                  <div
                    style={{
                      fontSize: 12,
                      fontWeight: 700,
                      color: 'var(--primary-strong)',
                      marginBottom: 4,
                    }}
                  >
                    🧭 新路线已生成
                  </div>
                  <div
                    style={{
                      fontSize: 11,
                      color: 'var(--fg-muted)',
                      lineHeight: 1.4,
                      marginBottom: 8,
                    }}
                  >
                    {m.route.summary?.slice(0, 60)}...
                    <br />
                    <strong style={{ color: 'var(--primary-strong)' }}>
                      {m.route.stops.length} 馆 ·{' '}
                      {Math.round(m.route.total_minutes / 60 * 10) / 10}h
                    </strong>
                  </div>
                  <button
                    className="pill-btn primary"
                    style={{ width: '100%', justifyContent: 'center' }}
                    onClick={onGoPlan}
                  >
                    📍 查看完整路线 →
                  </button>
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

      {/* Quick replies */}
      <div className="quick-feedback">
        {QUICK.map((q) => (
          <button key={q} onClick={() => send(q)} disabled={loading}>
            {q}
          </button>
        ))}
      </div>

      {/* Input */}
      <div className="chat-input">
        <input
          type="text"
          value={input}
          onChange={(e) => setInput(e.target.value)}
          onKeyDown={(e) => e.key === 'Enter' && send()}
          placeholder="说点什么…"
          disabled={loading}
        />
        <button className="btn btn-primary" onClick={() => send()} disabled={loading || !input.trim()}>
          发送
        </button>
      </div>

      {messages.length > 1 && (
        <button
          onClick={reset}
          style={{
            fontSize: 11,
            color: 'var(--fg-muted)',
            background: 'transparent',
            border: 'none',
            textAlign: 'center',
            width: '100%',
            padding: 8,
          }}
        >
          🗑 清空对话
        </button>
      )}

      {!currentRoute && (
        <button
          onClick={onGoActivity}
          style={{
            fontSize: 12,
            color: 'var(--primary-strong)',
            background: 'var(--primary-soft)',
            border: 'none',
            textAlign: 'center',
            width: '100%',
            padding: 10,
            borderRadius: 10,
            marginTop: 4,
          }}
        >
          📍 没规划？先去附近逛逛 / 📸 拍张照
        </button>
      )}
    </div>
  )
}