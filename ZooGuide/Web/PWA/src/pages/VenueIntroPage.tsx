import { useNavigate } from 'react-router-dom'
import type { Venue } from '../types'
import { venueEmoji } from '../lib/venue-helpers'

interface Props {
  venues: Venue[]
  meta?: any
}

export function VenueIntroPage({ venues, meta }: Props) {
  const navigate = useNavigate()

  const AREA_ICONS: Record<string, string> = { ...(meta?.area_icons || {}) }

  const areas = Object.entries(
    venues.reduce<Record<string, Venue[]>>((acc, v) => {
      const area = v.area || '其他'
      if (!acc[area]) acc[area] = []
      acc[area].push(v)
      return acc
    }, {})
  )

  return (
    <div className="fullscreen-flow">
      <header className="flow-header">
        <button className="flow-back" onClick={() => navigate('/')}>←</button>
        <div className="flow-title">🗺️ 场馆导览</div>
        <div style={{ width: 36 }} />
      </header>

      <div className="flow-body">
        <div className="venue-intro-summary">
          {venues.length} 个展馆 · {venues.filter(v => v.must_see).length} 个必看 · 按片区浏览
        </div>

        {areas.map(([area, areaVenues]) => (
          <div key={area} className="venue-area-section">
            <div className="venue-area-header">
              <span className="venue-area-icon">{AREA_ICONS[area] || '📍'}</span>
              <span className="venue-area-name">{area}</span>
              <span className="venue-area-count">{areaVenues.length} 馆</span>
            </div>
            <div className="venue-area-grid">
              {areaVenues.map((v) => (
                <button
                  key={v.id}
                  className={`venue-intro-card ${v.must_see ? 'must-see' : ''}`}
                  onClick={() => navigate(`/venue/${v.id}`)}
                >
                  <div className="vic-emoji">{venueEmoji(v.id, meta)}</div>
                  <div className="vic-name">{v.name}</div>
                  <div className="vic-animals">
                    {v.animals.slice(0, 2).join('·')}
                    {v.animals.length > 2 ? '…' : ''}
                  </div>
                  <div className="vic-meta">
                    <span>{v.recommended_visit_minutes}min</span>
                    {v.must_see && <span className="vic-badge">必看</span>}
                    {v.tags.includes('2025新馆') && <span className="vic-badge new">新馆</span>}
                  </div>
                </button>
              ))}
            </div>
          </div>
        ))}
      </div>
    </div>
  )
}
