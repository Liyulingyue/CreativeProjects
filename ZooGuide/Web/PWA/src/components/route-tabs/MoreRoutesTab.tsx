import { useEffect, useState } from 'react'
import { api } from '../../api/client'
import type { Route, UserPreference } from '../../types'
import { loadVisited } from '../../lib/storage'

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
  const [confirmVariant, setConfirmVariant] = useState<Variant | null>(null)

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

  function computeDiff(variant: Variant) {
    const visited = loadVisited()
    const currentIds = currentRoute.stops.map((s) => s.venue_id)
    const variantIds = variant.stops.map((s) => s.venue_id)

    const visitedIds = currentRoute.stops
      .filter((s) => visited.has(s.venue_id))
      .map((s) => s.venue_id)

    const kept: string[] = []
    const removed: string[] = []
    const added: string[] = []

    for (const id of visitedIds) {
      if (variantIds.includes(id)) kept.push(id)
      else removed.push(id)
    }
    for (const id of variantIds) {
      if (!currentIds.includes(id) || (!visitedIds.includes(id) && !removed.includes(id) && !kept.includes(id))) {
        if (!kept.includes(id) && !removed.includes(id)) added.push(id)
      }
    }

    const addedClean = variantIds.filter((id) => !currentIds.includes(id))

    return { kept, removed, added: addedClean }
  }

  function getVenueName(id: string, route: Route): string {
    const stop = route.stops.find((s) => s.venue_id === id)
    return stop?.venue_name || id
  }

  function handleApply(variant: Variant) {
    setConfirmVariant(variant)
  }

  function confirmApply() {
    if (!confirmVariant) return
    onApplyVariant(confirmVariant)
    setConfirmVariant(null)
  }

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
              onClick={() => handleApply(v)}
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

      {confirmVariant && (() => {
        const diff = computeDiff(confirmVariant)
        return (
          <div className="modal-mask" onClick={() => setConfirmVariant(null)}>
            <div className="modal" onClick={(e) => e.stopPropagation()}>
              <h3>确认切换方案</h3>
              <div style={{ fontSize: 13, color: 'var(--fg-muted)', marginBottom: 14, lineHeight: 1.6 }}>
                应用「{confirmVariant.variant_label || '新方案'}」会替换当前路线：
              </div>

              {diff.kept.length > 0 && (
                <div style={{ marginBottom: 8 }}>
                  <div style={{ fontSize: 12, color: '#10b981', fontWeight: 700, marginBottom: 4 }}>✅ 保留已游览</div>
                  <div style={{ fontSize: 12, color: 'var(--fg)', lineHeight: 1.6 }}>
                    {diff.kept.map((id) => getVenueName(id, currentRoute)).join('、')}
                  </div>
                </div>
              )}

              {diff.removed.length > 0 && (
                <div style={{ marginBottom: 8 }}>
                  <div style={{ fontSize: 12, color: '#ef4444', fontWeight: 700, marginBottom: 4 }}>❌ 已游览但不在新方案中</div>
                  <div style={{ fontSize: 12, color: 'var(--fg)', lineHeight: 1.6 }}>
                    {diff.removed.map((id) => getVenueName(id, currentRoute)).join('、')}
                  </div>
                </div>
              )}

              {diff.added.length > 0 && (
                <div style={{ marginBottom: 8 }}>
                  <div style={{ fontSize: 12, color: '#3b82f6', fontWeight: 700, marginBottom: 4 }}>🆕 新增场馆</div>
                  <div style={{ fontSize: 12, color: 'var(--fg)', lineHeight: 1.6 }}>
                    {diff.added.map((id) => getVenueName(id, confirmVariant)).join('、')}
                  </div>
                </div>
              )}

              <div style={{ fontSize: 12, color: 'var(--fg-muted)', marginTop: 8, marginBottom: 14, display: 'flex', gap: 16 }}>
                <span>⏱️ {Math.round(currentRoute.total_minutes / 60 * 10) / 10}h → {Math.round(confirmVariant.total_minutes / 60 * 10) / 10}h</span>
                <span>🚶 {Math.round(currentRoute.total_walk_minutes)}→{Math.round(confirmVariant.total_walk_minutes)}分钟步行</span>
                <span>📍 {currentRoute.stops.length}→{confirmVariant.stops.length}馆</span>
              </div>

              <div className="modal-actions">
                <button className="btn btn-ghost" onClick={() => setConfirmVariant(null)}>取消</button>
                <button className="btn btn-primary" onClick={confirmApply}>确认应用</button>
              </div>
            </div>
          </div>
        )
      })()}
    </div>
  )
}