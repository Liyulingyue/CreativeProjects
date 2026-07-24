import { useEffect, useRef, useState, useCallback } from 'react'
import type { Route, UserPreference } from '../types'
import {
  loadChatHistory,
  saveChatHistory,
  clearChatHistory as clearStorageChat,
  type ChatMessage,
} from '../lib/storage'

interface Props {
  currentRoute: Route | null
  prefs: UserPreference | null
  onRouteUpdate: (r: Route) => void
  onGoPlan: () => void
  onGoActivity: () => void
}

interface DisplayMsg {
  id: number
  role: 'user' | 'agent'
  text: string
  toolCalls?: { name: string; result: string }[]
  routeChanged?: boolean
}

const QUICK = [
  '走不动了',
  '太晒了',
  '加考拉馆',
  '跳老虎',
  '想多逛',
  '看网红',
]

const WELCOME: DisplayMsg = {
  id: 0,
  role: 'agent',
  text: '嗨，我是你的红山导游。想逛哪些馆？走累了？想看什么动物？随时告诉我。',
}

export function ChatPage({ currentRoute, prefs, onRouteUpdate, onGoPlan, onGoActivity }: Props) {
  const [messages, setMessages] = useState<DisplayMsg[]>(() => {
    const stored = loadChatHistory()
    if (stored.length === 0) return [WELCOME]
    return stored.map((m, i) => ({
      id: i,
      role: m.role === 'assistant' ? 'agent' : 'user',
      text: m.content,
      toolCalls: m.toolCalls,
      routeChanged: !!m.newRoute,
    }))
  })
  const [input, setInput] = useState('')
  const [loading, setLoading] = useState(false)
  const [agentSteps, setAgentSteps] = useState<string[]>([])
  const [showQuick, setShowQuick] = useState(true)
  const scrollerRef = useRef<HTMLDivElement>(null)
  const idRef = useRef(messages.length + 1)

  useEffect(() => {
    setTimeout(() => {
      scrollerRef.current?.scrollTo({ top: 999999, behavior: 'smooth' })
    }, 50)
  }, [messages, loading, agentSteps])

  const persistMessages = useCallback((msgs: DisplayMsg[]) => {
    const chatMsgs: ChatMessage[] = msgs
      .filter((m) => m.id !== 0)
      .map((m) => ({
        role: m.role === 'agent' ? 'assistant' : ('user' as const),
        content: m.text,
        toolCalls: m.toolCalls,
        newRoute: m.routeChanged ? {} : undefined,
      }))
    saveChatHistory(chatMsgs)
  }, [])

  async function send(text?: string) {
    const msg = (text ?? input).trim()
    if (!msg || loading) return
    setInput('')

    const userMsg: DisplayMsg = { id: idRef.current++, role: 'user', text: msg }
    const nextMsgs = [...messages, userMsg]
    setMessages(nextMsgs)
    setLoading(true)
    setAgentSteps([])

    const history: ChatMessage[] = nextMsgs
      .filter((m) => m.id !== 0)
      .map((m) => ({
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

      const agentMsg: DisplayMsg = {
        id: idRef.current++,
        role: 'agent',
        text: d.reply || '…',
        routeChanged: !!d.new_route,
      }
      const updated = [...nextMsgs, agentMsg]
      setMessages(updated)
      persistMessages(updated)

      if (d.new_route && onRouteUpdate) {
        onRouteUpdate(d.new_route)
      }
    } catch {
      const errMsg: DisplayMsg = {
        id: idRef.current++,
        role: 'agent',
        text: '网络好像出问题了，试试再说一次？',
      }
      const updated = [...nextMsgs, errMsg]
      setMessages(updated)
      persistMessages(updated)
    } finally {
      setLoading(false)
      setAgentSteps([])
    }
  }

  function reset() {
    setMessages([WELCOME])
    idRef.current = 1
    clearStorageChat()
  }

  return (
    <div className="chat-page">
      <div className="chat-context">
        {currentRoute ? (
          <>
            <div className="chat-context-info">
              <div className="chat-context-label">当前路线</div>
              <div className="chat-context-meta">
                {currentRoute.stops.length} 馆 ·{' '}
                {Math.round((currentRoute.total_minutes / 60) * 10) / 10}h
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

      <div ref={scrollerRef} className="chat-messages">
        {messages.map((m) => (
          <MessageBubble key={m.id} msg={m} />
        ))}
        {loading && agentSteps.length > 0 && (
          <div className="chat-agent-steps">
            {agentSteps.map((s, i) => (
              <div key={i} className="chat-agent-step">{s}</div>
            ))}
          </div>
        )}
        {loading && <TypingBubble />}
      </div>

      {showQuick && (
        <div className="chat-quick-row">
          <button className="chat-quick-chip chat-quick-new" onClick={reset} disabled={loading}>
            新对话
          </button>
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
    </div>
  )
}

function MessageBubble({ msg }: { msg: DisplayMsg }) {
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
          boxShadow: isUser
            ? '0 1px 4px rgba(45,106,79,0.25)'
            : '0 1px 2px rgba(0,0,0,0.04)',
        }}
      >
        {msg.text}
        {msg.routeChanged && (
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
