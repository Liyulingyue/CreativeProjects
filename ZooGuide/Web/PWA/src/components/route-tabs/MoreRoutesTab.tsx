import { useEffect, useState } from 'react'
import { api } from '../../api/client'
import type { Route, UserPreference } from '../../types'

interface Props {
  prefs: UserPreference | null
  currentRoute: Route
  onApplyVariant: (r: Route) => void
}

interface Variant extends Route {
  variant_label?: string
}

export function MoreRoutesTab({ prefs, currentRoute, onApplyVariant }: Props) {
  const [variants, setVariants] = useState<Variant[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)

  async function load() {
    if (!prefs) return
    setLoading(true)
    setError(null)
    try {
      const d = await api.planVariants(prefs)
      setVariants(d.variants as Variant[])
    } catch (e) {
      setError(e instanceof Error ? e.message : '加载失败')
    } finally {
      setLoading(false)
    }
  }

  useEffect(() => {
    load()
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [prefs?.available_hours, prefs?.party_type, prefs?.entry_gate])

  return (
    <div className="more-tab">
      <div
        style={{
          display: 'flex',
          alignItems: 'center',
          gap: 8,
          marginBottom: 12,
        }}
      >
        <div style={{ flex: 1, fontSize: 13, color: 'var(--fg-muted)' }}>
          预生成的 {variants.length} 条路线，1 键应用
        </div>
        <button className="pill-btn" onClick={load} disabled={loading}>
          {loading ? '生成中…' : '🔄 换一批'}
        </button>
      </div>

      {loading && (
        <div className="loading">
          <div className="spinner" />
          生成对比路线…
        </div>
      )}

      {error && <div className="error-banner">{error}</div>}

      {variants.map((v, i) => {
        const sameAsCurrent =
          JSON.stringify(v.stops.map((s) => s.venue_id)) ===
          JSON.stringify(currentRoute.stops.map((s) => s.venue_id))
        return (
          <div
            key={i}
            className="variant-card"
            style={sameAsCurrent ? { borderColor: 'var(--primary)', opacity: 0.85 } : undefined}
          >
            <div
              style={{
                display: 'flex',
                alignItems: 'center',
                gap: 8,
                marginBottom: 6,
              }}
            >
              <span
                style={{
                  fontSize: 16,
                  fontWeight: 700,
                  color: 'var(--primary-strong)',
                }}
              >
                {v.variant_label || `方案 ${i + 1}`}
              </span>
              {sameAsCurrent && (
                <span
                  style={{
                    fontSize: 10,
                    background: 'var(--primary)',
                    color: 'white',
                    padding: '2px 8px',
                    borderRadius: 999,
                    fontWeight: 600,
                  }}
                >
                  当前
                </span>
              )}
              <span
                style={{
                  marginLeft: 'auto',
                  fontSize: 11,
                  color: 'var(--fg-muted)',
                }}
              >
                {v.stops.length} 馆 · {Math.round(v.total_minutes / 60 * 10) / 10}h
              </span>
            </div>

            <div
              style={{
                fontSize: 12,
                color: 'var(--fg-muted)',
                lineHeight: 1.5,
                marginBottom: 8,
              }}
            >
              {v.summary}
            </div>

            <div
              style={{
                fontSize: 11,
                color: 'var(--primary-strong)',
                marginBottom: 10,
                padding: '6px 8px',
                background: 'var(--bg)',
                borderRadius: 6,
              }}
            >
              {v.stops.map((s) => s.venue_name).join(' → ')}
            </div>

            <button
              className="btn btn-primary btn-full"
              disabled={sameAsCurrent}
              style={sameAsCurrent ? { background: '#9bb5a5', cursor: 'not-allowed' } : undefined}
              onClick={() => onApplyVariant(v)}
            >
              {sameAsCurrent ? '✓ 当前方案' : '📍 应用此方案'}
            </button>
          </div>
        )
      })}

      {!loading && variants.length === 0 && (
        <div className="card" style={{ textAlign: 'center', color: 'var(--fg-muted)' }}>
          暂无可用方案
        </div>
      )}
    </div>
  )
}