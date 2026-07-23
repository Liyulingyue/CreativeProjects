import { useEffect, useState } from 'react'
import { useNavigate, useParams } from 'react-router-dom'
import type { Venue } from '../types'
import { api } from '../api/client'

const VENUE_EMOJIS: Record<string, string> = {
  panda: '🐼', koala: '🐨', gorilla: '🦍', tiger: '🐯',
  china_cat: '🐆', cat_planet: '🐱', giraffe: '🦒', asian_elephant: '🐘',
  orangutan: '🦧', asian_primates: '🐒', red_panda: '🐾', kangaroo: '🦘',
  lemur: '🦝', rhino: '🦏', hornbill: '🦜', crane: '🦢',
  wolf: '🐺', bear: '🐻', monkey_mountain: '🐵', meerkat: '🦡',
  tangjiahe: '🏞️', gonwana: '🦎', dazhuangguange: '🏛️',
}

export function VenueDetailPage() {
  const { id } = useParams<{ id: string }>()
  const navigate = useNavigate()
  const [venue, setVenue] = useState<Venue | null>(null)
  const [loading, setLoading] = useState(true)

  useEffect(() => {
    if (!id) return
    api
      .venue(id)
      .then((d) => setVenue(d as Venue))
      .catch(() => {})
      .finally(() => setLoading(false))
  }, [id])

  if (loading) {
    return (
      <div className="fullscreen-flow">
        <header className="flow-header">
          <button className="flow-back" onClick={() => navigate('/venue')}>←</button>
          <div className="flow-title">加载中…</div>
          <div style={{ width: 36 }} />
        </header>
        <div className="flow-body" style={{ textAlign: 'center', padding: 40, color: 'var(--fg-muted)' }}>
          加载中…
        </div>
      </div>
    )
  }

  if (!venue) {
    return (
      <div className="fullscreen-flow">
        <header className="flow-header">
          <button className="flow-back" onClick={() => navigate('/venue')}>←</button>
          <div className="flow-title">未找到</div>
          <div style={{ width: 36 }} />
        </header>
        <div className="flow-body" style={{ textAlign: 'center', padding: 40, color: 'var(--fg-muted)' }}>
          该场馆不存在
        </div>
      </div>
    )
  }

  const emoji = VENUE_EMOJIS[venue.id] || '🏠'

  return (
    <div className="fullscreen-flow">
      <header className="flow-header">
        <button className="flow-back" onClick={() => navigate('/venue')}>←</button>
        <div className="flow-title">{emoji} {venue.name}</div>
        <div style={{ width: 36 }} />
      </header>

      <div className="flow-body">
        <div className="venue-detail-hero-section">
          <div className="vdh-emoji">{emoji}</div>
          <div className="vdh-name">{venue.name}</div>
          <div className="vdh-area">{venue.area}</div>
          {venue.must_see && <div className="vdh-must-see">⭐ 必看场馆</div>}
        </div>

        {venue.description && (
          <div className="card">
            <div className="vd-section-title">📋 场馆简介</div>
            <p className="vd-text">{venue.description}</p>
          </div>
        )}

        {venue.narration && (
          <div className="card" style={{ borderLeft: '3px solid var(--primary)' }}>
            <div className="vd-section-title">📖 讲解词</div>
            <p className="vd-text narration">{venue.narration}</p>
          </div>
        )}

        <div className="card">
          <div className="vd-section-title">🦎 动物居民</div>
          <div className="vd-animal-list">
            {venue.animals.map((a, i) => (
              <span key={i} className="vd-animal-tag">{a}</span>
            ))}
          </div>
          {venue.animals.length === 0 && (
            <div className="vd-empty">此场馆暂无动物展示</div>
          )}
        </div>

        <div className="card">
          <div className="vd-section-title">📋 基本信息</div>
          <div className="vd-info-grid">
            <div className="vd-info-item">
              <span className="vd-info-label">开放时间</span>
              <span className="vd-info-value">{venue.open_time}–{venue.close_time}</span>
            </div>
            <div className="vd-info-item">
              <span className="vd-info-label">建议游览</span>
              <span className="vd-info-value">{venue.recommended_visit_minutes} 分钟</span>
            </div>
            <div className="vd-info-item">
              <span className="vd-info-label">亲子友好</span>
              <span className="vd-info-value">{'⭐'.repeat(venue.kid_friendly)}</span>
            </div>
            <div className="vd-info-item">
              <span className="vd-info-label">出片指数</span>
              <span className="vd-info-value">{'📸'.repeat(venue.photo_op)}</span>
            </div>
            <div className="vd-info-item">
              <span className="vd-info-label">有遮阴</span>
              <span className="vd-info-value">{venue.shaded ? '✅' : '❌'}</span>
            </div>
            <div className="vd-info-item">
              <span className="vd-info-label">休息区</span>
              <span className="vd-info-value">{venue.rest_spots ? '✅' : '❌'}</span>
            </div>
            {venue.keeper_talk && (
              <div className="vd-info-item">
                <span className="vd-info-label">饲养员讲解</span>
                <span className="vd-info-value">{venue.keeper_talk}</span>
              </div>
            )}
          </div>
        </div>

        {venue.seasonal_tips && (
          <div className="card" style={{ background: '#fef9e7' }}>
            <div className="vd-section-title">🌤️ 四季小贴士</div>
            <p className="vd-text" style={{ color: '#78350f' }}>{venue.seasonal_tips}</p>
          </div>
        )}

        {venue.tags.length > 0 && (
          <div className="card">
            <div className="vd-section-title">🏷️ 标签</div>
            <div className="vd-tag-list">
              {venue.tags.map((t) => (
                <span key={t} className="facility-tag">{t}</span>
              ))}
            </div>
          </div>
        )}
      </div>
    </div>
  )
}
