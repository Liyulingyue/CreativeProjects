import { useEffect, useRef, useState } from 'react'

interface Props {
  onClose: () => void
  onNewRoute?: (route: any) => void
  currentRoute?: any
  prefs?: any
}

interface ChatMsg {
  role: 'user' | 'agent'
  text: string
  constraint?: any
  route?: any
}

export function ChatDialog({ onClose, onNewRoute, currentRoute, prefs }: Props) {
  const [messages, setMessages] = useState<ChatMsg[]>([
    {
      role: 'agent',
      text: '嗨，我是你的红山导游。走累了？晒了？想换路线？随时告诉我。',
    },
  ])
  const [input, setInput] = useState('')
  const [loading, setLoading] = useState(false)
  const scrollerRef = useRef<HTMLDivElement>(null)

  useEffect(() => {
    scrollerRef.current?.scrollTo({ top: 99999, behavior: 'smooth' })
  }, [messages])

  const QUICK = [
    '走不动了，能少走点吗？',
    '太阳太晒，能换阴凉的路线吗',
    '孩子累了想坐一会儿',
    '想多看几个馆',
    '想换条不一样的路线',
    '想看网红动物',
  ]

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
        route: d.new_route,
      }
      setMessages((prev) => [...prev, reply])
      if (d.new_route && onNewRoute) {
        onNewRoute(d.new_route)
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
                {m.constraint && (
                  <div
                    style={{
                      marginTop: 6,
                      fontSize: 11,
                      opacity: 0.8,
                    }}
                  >
                    💡 已识别：{m.constraint.type}
                  </div>
                )}
                {m.route && (
                  <div
                    style={{
                      marginTop: 6,
                      fontSize: 12,
                      color: 'var(--primary-strong)',
                      fontWeight: 600,
                    }}
                  >
                    ✓ 后半段已重新规划
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