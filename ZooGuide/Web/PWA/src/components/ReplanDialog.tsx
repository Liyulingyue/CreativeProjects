import { useState } from 'react'
import type { Route, UserPreference } from '../types'
import { api } from '../api/client'

interface Props {
  route: Route
  prefs: UserPreference
  currentStopIdx: number
  elapsedMinutes: number
  onClose: () => void
  onApplied: (r: Route) => void
}

const QUICK_FEEDBACKS = [
  '走不动了，能少走点吗？',
  '太晒了，能不能多去阴凉的地方',
  '想多看几个场馆',
  '孩子饿了想找地方休息',
  '想看更多网红动物',
  '太冷了想多去室内',
]

export function ReplanDialog({ route, currentStopIdx, elapsedMinutes, onClose, onApplied }: Props) {
  const [feedback, setFeedback] = useState(QUICK_FEEDBACKS[0])
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)

  async function submit() {
    setLoading(true)
    setError(null)
    try {
      const currentStop = route.stops[currentStopIdx]
      const updated = await api.replan({
        original_route: route,
        current_venue_id: currentStop?.venue_id,
        elapsed_minutes: elapsedMinutes,
        feedback,
      })
      onApplied(updated)
    } catch (e) {
      setError(e instanceof Error ? e.message : '重新规划失败')
    } finally {
      setLoading(false)
    }
  }

  return (
    <div className="modal-mask" onClick={onClose}>
      <div className="modal" onClick={(e) => e.stopPropagation()}>
        <h3>✨ 重新规划后半段</h3>
        <p style={{ fontSize: 13, color: 'var(--fg-muted)', margin: '0 0 12px' }}>
          当前在 <strong>{route.stops[currentStopIdx]?.venue_name || '尚未开始'}</strong>，
          已用 <strong>{elapsedMinutes} 分钟</strong>。把下面的感受告诉 Agent，剩下的交给它。
        </p>

        <div className="quick-feedback">
          {QUICK_FEEDBACKS.map((q) => (
            <button
              key={q}
              className={feedback === q ? 'active' : ''}
              onClick={() => setFeedback(q)}
            >
              {q}
            </button>
          ))}
        </div>

        <textarea
          value={feedback}
          onChange={(e) => setFeedback(e.target.value)}
          placeholder="或者直接说说你现在的感受…"
        />

        {error && <div className="error-banner" style={{ marginTop: 12 }}>{error}</div>}

        <div className="modal-actions">
          <button className="btn btn-ghost" onClick={onClose} disabled={loading}>
            取消
          </button>
          <button className="btn btn-primary" onClick={submit} disabled={loading || !feedback.trim()}>
            {loading ? '规划中…' : '✨ 重新规划'}
          </button>
        </div>
      </div>
    </div>
  )
}