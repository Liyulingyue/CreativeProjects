import { useEffect, useState } from 'react'
import { useNavigate, useParams } from 'react-router-dom'
import type { Facility } from '../types'
import { api } from '../api/client'

const CATEGORY_ICONS: Record<string, string> = {
  卫生间: '🚻',
  餐饮: '🍜',
  售票: '🎫',
  商店: '🛍️',
  停车: '🅿️',
  医疗: '🏥',
  寄存: '🧳',
  母婴: '🍼',
}

export function FacilityDetailPage() {
  const { id } = useParams<{ id: string }>()
  const navigate = useNavigate()
  const [facility, setFacility] = useState<Facility | null>(null)
  const [loading, setLoading] = useState(true)

  useEffect(() => {
    if (!id) return
    api
      .facility(id)
      .then((d) => setFacility(d as Facility))
      .catch(() => {})
      .finally(() => setLoading(false))
  }, [id])

  if (loading) {
    return (
      <div className="fullscreen-flow">
        <header className="flow-header">
          <button className="flow-back" onClick={() => navigate('/facility')}>←</button>
          <div className="flow-title">加载中…</div>
          <div style={{ width: 36 }} />
        </header>
        <div className="flow-body" style={{ textAlign: 'center', padding: 40, color: 'var(--fg-muted)' }}>
          加载中…
        </div>
      </div>
    )
  }

  if (!facility) {
    return (
      <div className="fullscreen-flow">
        <header className="flow-header">
          <button className="flow-back" onClick={() => navigate('/facility')}>←</button>
          <div className="flow-title">未找到</div>
          <div style={{ width: 36 }} />
        </header>
        <div className="flow-body" style={{ textAlign: 'center', padding: 40, color: 'var(--fg-muted)' }}>
          该设施不存在
        </div>
      </div>
    )
  }

  const icon = CATEGORY_ICONS[facility.category] || '📍'

  return (
    <div className="fullscreen-flow">
      <header className="flow-header">
        <button className="flow-back" onClick={() => navigate('/facility')}>←</button>
        <div className="flow-title">{icon} {facility.name}</div>
        <div style={{ width: 36 }} />
      </header>

      <div className="flow-body">
        <div className="facility-detail-hero">
          <div className="facility-detail-icon">{icon}</div>
          <div style={{ fontSize: 18, fontWeight: 700, color: 'var(--primary-strong)' }}>
            {facility.name}
          </div>
          <div className="facility-detail-category">{facility.category} · {facility.area}</div>
        </div>

        {facility.description && (
          <div className="card" style={{ marginTop: 16 }}>
            <div style={{ fontSize: 13, fontWeight: 700, color: 'var(--primary-strong)', marginBottom: 8 }}>
              介绍
            </div>
            <p style={{ fontSize: 14, lineHeight: 1.7, color: 'var(--fg)', margin: 0 }}>
              {facility.description}
            </p>
          </div>
        )}

        <div className="card" style={{ marginTop: 12 }}>
          <div className="facility-info-grid">
            {facility.open_time && (
              <div className="facility-info-item">
                <span className="facility-info-label">开放时间</span>
                <span className="facility-info-value">{facility.open_time}</span>
              </div>
            )}
            {facility.near_venue_name && (
              <div className="facility-info-item">
                <span className="facility-info-label">附近场馆</span>
                <span className="facility-info-value">{facility.near_venue_name}</span>
              </div>
            )}
            {facility.area && (
              <div className="facility-info-item">
                <span className="facility-info-label">所在区域</span>
                <span className="facility-info-value">{facility.area}</span>
              </div>
            )}
          </div>
        </div>

        {facility.tags.length > 0 && (
          <div className="card" style={{ marginTop: 12 }}>
            <div style={{ fontSize: 13, fontWeight: 700, color: 'var(--primary-strong)', marginBottom: 8 }}>
              标签
            </div>
            <div style={{ display: 'flex', flexWrap: 'wrap', gap: 6 }}>
              {facility.tags.map((t) => (
                <span key={t} className="facility-tag">{t}</span>
              ))}
            </div>
          </div>
        )}
      </div>
    </div>
  )
}
