import { useEffect, useState } from 'react'
import { api } from '../api/client'
import type { Route, UserPreference } from '../types'

interface Props {
  prefs: UserPreference
  onClose: () => void
  onPick: (route: Route) => void
}

export function VariantsModal({ prefs, onClose, onPick }: Props) {
  const [variants, setVariants] = useState<any[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    api
      .planVariants(prefs)
      .then((d) => setVariants(d.variants))
      .catch((e) => setError(e instanceof Error ? e.message : '失败'))
      .finally(() => setLoading(false))
  }, [])

  return (
    <div className="modal-mask" onClick={onClose}>
      <div className="modal" onClick={(e) => e.stopPropagation()} style={{ maxWidth: 460 }}>
        <h3>🧭 三种逛法，你选哪条？</h3>
        {loading && (
          <div className="loading">
            <div className="spinner" />
            生成对比路线…
          </div>
        )}
        {error && <div className="error-banner">{error}</div>}
        {variants.map((v, i) => (
          <div
            key={i}
            className="card"
            style={{ marginBottom: 10, cursor: 'pointer' }}
            onClick={() => {
              onPick(v)
              onClose()
            }}
          >
            <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 6 }}>
              <span style={{ fontSize: 16, fontWeight: 700, color: 'var(--primary-strong)' }}>
                {v.variant_label || '方案'}
              </span>
              <span
                style={{
                  fontSize: 11,
                  color: 'var(--fg-muted)',
                  background: 'var(--primary-soft)',
                  padding: '2px 8px',
                  borderRadius: 6,
                }}
              >
                {v.stops.length} 馆 · {Math.round(v.total_minutes / 60 * 10) / 10}h
              </span>
            </div>
            <div style={{ fontSize: 13, color: 'var(--fg-muted)', marginBottom: 8, lineHeight: 1.5 }}>
              {v.summary}
            </div>
            <div style={{ fontSize: 12, color: 'var(--primary-strong)' }}>
              {v.stops.map((s: any) => s.venue_name).join(' → ')}
            </div>
          </div>
        ))}
        <div className="modal-actions">
          <button className="btn btn-ghost btn-full" onClick={onClose}>
            取消
          </button>
        </div>
      </div>
    </div>
  )
}