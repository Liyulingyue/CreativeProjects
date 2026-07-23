import { useNavigate } from 'react-router-dom'
import type { Venue } from '../types'

const AREA_ICONS: Record<string, string> = {
  '大红山片区': '🏔️',
  '放牛山片区': '🌿',
  '小红山片区': '🦅',
  '南门新区': '🌍',
}

const VENUE_EMOJIS: Record<string, string> = {
  panda: '🐼', koala: '🐨', gorilla: '🦍', tiger: '🐯',
  china_cat: '🐆', cat_planet: '🐱', giraffe: '🦒', asian_elephant: '🐘',
  orangutan: '🦧', asian_primates: '🐒', red_panda: '🐾', kangaroo: '🦘',
  lemur: '🦝', rhino: '🦏', hornbill: '🦜', crane: '🦢',
  wolf: '🐺', bear: '🐻', monkey_mountain: '🐵', meerkat: '🦡',
  tangjiahe: '🏞️', gonwana: '🦎', dazhuangguange: '🏛️',
}

interface Props {
  venues: Venue[]
}

export function VenueIntroPage({ venues }: Props) {
  const navigate = useNavigate()

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
                  <div className="vic-emoji">{VENUE_EMOJIS[v.id] || '🏠'}</div>
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
