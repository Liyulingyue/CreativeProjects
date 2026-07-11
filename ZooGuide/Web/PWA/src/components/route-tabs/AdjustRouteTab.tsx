import { useState } from 'react'
import { api } from '../../api/client'
import type { Route } from '../../types'

interface Props {
  currentRoute: Route | null
  currentStopIdx: number
  elapsedMinutes: number
  prefs: any
  onReplanned: (r: Route) => void
  onRestartQuiz?: () => void
  onOpenChat?: () => void
}

const QUICK = [
  '走不动了，能少走点吗？',
  '太阳太晒，换阴凉的路线',
  '加上考拉馆',
  '跳过老虎',
  '帮我多看几个馆',
  '想看网红动物',
]

export function AdjustRouteTab({
  currentRoute,
  currentStopIdx,
  elapsedMinutes,
  prefs,
  onReplanned,
  onRestartQuiz,
  onOpenChat,
}: Props) {
  const [message, setMessage] = useState('')
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [lastResult, setLastResult] = useState<{ reply: string; route?: Route } | null>(null)

  async function adjust(text?: string) {
    const msg = (text ?? message).trim()
    if (!msg || loading || !currentRoute) return
    setMessage('')
    setLoading(true)
    setError(null)
    try {
      // Use chat endpoint for both adjust + replan
      const r = await fetch('/api/chat', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          message: msg,
          current_route: currentRoute,
          prefs,
          history: [],
        }),
      })
      const d = await r.json()
      setLastResult({ reply: d.reply, route: d.new_route })
      if (d.new_route) {
        onReplanned(d.new_route)
      } else if (d.suggested_replan) {
        // No new route generated, but suggested — fallback to /replan
        const replanRes = await fetch('/api/replan', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({
            original_route: currentRoute,
            current_venue_id: currentRoute.stops[currentStopIdx]?.venue_id,
            elapsed_minutes: elapsedMinutes,
            feedback: msg,
          }),
        })
        const rd = await replanRes.json()
        if (rd && rd.id) {
          setLastResult({ reply: d.reply, route: rd })
          onReplanned(rd)
        }
      }
    } catch (e) {
      setError(e instanceof Error ? e.message : '调整失败')
    } finally {
      setLoading(false)
    }
  }

  return (
    <div className="adjust-tab">
      <div className="card" style={{ background: 'linear-gradient(135deg, var(--primary-soft), #fff)' }}>
        <h3 style={{ margin: '0 0 4px', color: 'var(--primary-strong)', fontSize: 16 }}>
          💬 一句话调整
        </h3>
        <p style={{ fontSize: 12, color: 'var(--fg-muted)', margin: '0 0 12px' }}>
          告诉 Agent 你现在的感受，Agent 帮你重新规划后半段
        </p>

        <textarea
          value={message}
          onChange={(e) => setMessage(e.target.value)}
          placeholder='例如："孩子累了想坐一会儿"、"想加上考拉馆"'
          disabled={loading}
          style={{ marginBottom: 10 }}
          rows={3}
        />

        <button
          className="btn btn-primary btn-full"
          onClick={() => adjust()}
          disabled={loading || !message.trim()}
        >
          {loading ? '调整中…' : '✨ 重新生成后半段'}
        </button>

        <div style={{ marginTop: 12 }}>
          <div style={{ fontSize: 11, color: 'var(--fg-muted)', marginBottom: 6 }}>
            快捷回复：
          </div>
          <div className="quick-feedback">
            {QUICK.map((q) => (
              <button key={q} onClick={() => adjust(q)} disabled={loading}>
                {q}
              </button>
            ))}
          </div>
        </div>
      </div>

      {lastResult && (
        <div
          className="card"
          style={{ marginTop: 12, background: '#f0fdf4', border: '1px solid #86efac' }}
        >
          <div style={{ fontSize: 13, color: '#15803d', fontWeight: 600, marginBottom: 6 }}>
            ✓ Agent 回复
          </div>
          <div style={{ fontSize: 13, color: '#1a3a2a', lineHeight: 1.5 }}>{lastResult.reply}</div>
          {lastResult.route && (
            <div style={{ fontSize: 11, color: '#15803d', marginTop: 6 }}>
              🧭 后半段已重新规划，{lastResult.route.stops.length} 馆
            </div>
          )}
        </div>
      )}

      <div className="card" style={{ marginTop: 14 }}>
        <h3 style={{ margin: '0 0 4px', color: 'var(--primary-strong)', fontSize: 16 }}>
          📋 重头来过
        </h3>
        <p style={{ fontSize: 12, color: 'var(--fg-muted)', margin: '0 0 12px' }}>
          重新填写问卷，从头生成路线
        </p>
        <button className="btn btn-outline btn-full" onClick={onRestartQuiz}>
          🔄 重新填问卷
        </button>
      </div>

      <div className="card" style={{ marginTop: 14 }}>
        <h3 style={{ margin: '0 0 4px', color: 'var(--primary-strong)', fontSize: 16 }}>
          💭 想详细聊聊？
        </h3>
        <p style={{ fontSize: 12, color: 'var(--fg-muted)', margin: '0 0 12px' }}>
          去对话 Tab，Agent 可以基于上下文回答你的问题
        </p>
        <button className="btn btn-ghost btn-full" onClick={onOpenChat}>
          💬 打开对话 →
        </button>
      </div>

      {error && <div className="error-banner" style={{ marginTop: 12 }}>{error}</div>}
    </div>
  )
}