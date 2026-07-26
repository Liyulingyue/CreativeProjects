import { useState } from 'react'
import { api } from '../../api/client'
import type { Route } from '../../types'
import { removeVisitedSourceAll } from '../../lib/storage'

interface Props {
  currentRoute: Route | null
  currentStopIdx: number
  elapsedMinutes: number
  prefs: any
  onReplanned: (r: Route) => void
  onRestartQuiz?: () => void
  onOpenChat?: () => void
  onResetProgress?: () => void
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
  onResetProgress,
}: Props) {
  const [message, setMessage] = useState('')
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [pendingRoute, setPendingRoute] = useState<Route | null>(null)

  async function adjust(text?: string) {
    const msg = (text ?? message).trim()
    if (!msg || loading || !currentRoute) return
    setMessage('')
    setLoading(true)
    setError(null)
    try {
      const currentStop = currentRoute.stops[currentStopIdx]
      const newRoute = await api.replan({
        original_route: currentRoute,
        current_venue_id: currentStop?.venue_id,
        elapsed_minutes: elapsedMinutes,
        feedback: msg,
      })
      setPendingRoute(newRoute)
    } catch (e) {
      setError(e instanceof Error ? e.message : '调整失败')
    } finally {
      setLoading(false)
    }
  }

  function confirmReplace() {
    if (pendingRoute) {
      onReplanned(pendingRoute)
      setPendingRoute(null)
    }
  }

  return (
    <div className="adjust-tab">
      <div className="card" style={{ background: 'linear-gradient(135deg, var(--primary-soft), #fff)' }}>
        <h3 style={{ margin: '0 0 4px', color: 'var(--primary-strong)', fontSize: 16 }}>
          💬 一句话调整
        </h3>
        <p style={{ fontSize: 12, color: 'var(--fg-muted)', margin: '0 0 12px' }}>
          告诉导游你现在的感受，帮你重新规划后半段
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
          {loading ? '重新规划中…' : '✨ 重新生成后半段'}
        </button>

        <div style={{ marginTop: 12 }}>
          <div style={{ fontSize: 11, color: 'var(--fg-muted)', marginBottom: 6 }}>
            快捷回复：
          </div>
          <div className="quick-feedback">
            {QUICK.map((q) => (
              <button key={q} onClick={() => setMessage(q)} disabled={loading}>
                {q}
              </button>
            ))}
          </div>
        </div>
      </div>

      {pendingRoute && (
        <div
          className="card"
          style={{ marginTop: 12, background: '#f0fdf4', border: '1px solid #86efac' }}
        >
          <div style={{ fontSize: 13, color: '#15803d', fontWeight: 600, marginBottom: 8 }}>
            🧭 新路线已生成，{pendingRoute.stops.length} 个场馆
          </div>
          <div style={{ fontSize: 12, color: '#1a3a2a', lineHeight: 1.6, marginBottom: 10 }}>
            {pendingRoute.stops.map((s, i) => (
              <div key={i}>
                {i + 1}. {s.venue_name}（{s.visit_minutes}分钟）
              </div>
            ))}
          </div>
          <div style={{ display: 'flex', gap: 8 }}>
            <button
              className="btn btn-ghost"
              style={{ flex: 1 }}
              onClick={() => setPendingRoute(null)}
            >
              取消
            </button>
            <button
              className="btn btn-primary"
              style={{ flex: 1 }}
              onClick={confirmReplace}
            >
              ✓ 采用新路线
            </button>
          </div>
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
        <button
          className="btn btn-ghost btn-full"
          style={{ marginTop: 8 }}
          onClick={() => {
            removeVisitedSourceAll('route')
            onResetProgress?.()
          }}
        >
          🔁 这条路线从头走
        </button>
      </div>

      <div className="card" style={{ marginTop: 14 }}>
        <h3 style={{ margin: '0 0 4px', color: 'var(--primary-strong)', fontSize: 16 }}>
          💭 想详细聊聊？
        </h3>
        <p style={{ fontSize: 12, color: 'var(--fg-muted)', margin: '0 0 12px' }}>
          去对话 Tab，导游可以基于上下文回答你的问题
        </p>
        <button className="btn btn-ghost btn-full" onClick={onOpenChat}>
          💬 打开对话 →
        </button>
      </div>

      {error && <div className="error-banner" style={{ marginTop: 12 }}>{error}</div>}
    </div>
  )
}
