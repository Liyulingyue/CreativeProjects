import { useNavigate } from 'react-router-dom'
import type { Meta, Venue } from '../types'

const VENUE_EMOJIS: Record<string, string> = {
  panda: '🐼', koala: '🐨', gorilla: '🦍', tiger: '🐯',
  china_cat: '🐆', cat_planet: '🐱', giraffe: '🦒', asian_elephant: '🐘',
  orangutan: '🦧', asian_primates: '🐒', red_panda: '🐾', kangaroo: '🦘',
  lemur: '🦝', rhino: '🦏', hornbill: '🦜', crane: '🦢',
  wolf: '🐺', bear: '🐻', monkey_mountain: '🐵', meerkat: '🦡',
  tangjiahe: '🏞️', gonwana: '🦎', dazhuangguange: '🏛️',
}

interface Props {
  meta: Meta | null
  venues: Venue[]
}

export function SeasonGuidePage({ meta, venues }: Props) {
  const navigate = useNavigate()
  const sg = meta?.seasonal_guide
  const venueMap = Object.fromEntries(venues.map(v => [v.id, v]))

  function renderSeason(key: 'peak' | 'shoulder' | 'off_peak') {
    if (!sg) return null
    const season = sg[key]
    const bestVenues = season.best_venues
      .map(vid => venueMap[vid])
      .filter(Boolean)

    const gradients: Record<string, string> = {
      peak: 'linear-gradient(135deg, #f59e0b, #d97706)',
      shoulder: 'linear-gradient(135deg, #10b981, #059669)',
      off_peak: 'linear-gradient(135deg, #3b82f6, #1d4ed8)',
    }

    const icons: Record<string, string> = {
      peak: '🔥',
      shoulder: '🌿',
      off_peak: '❄️',
    }

    return (
      <div className="season-card" key={key}>
        <div className="season-header" style={{ background: gradients[key], color: 'white', borderRadius: '14px 14px 0 0' }}>
          <div style={{ fontSize: 28 }}>{icons[key]}</div>
          <div style={{ fontSize: 15, fontWeight: 700 }}>{season.label}</div>
        </div>
        <div className="season-body">
          <ul className="season-tips-list">
            {season.tips.map((tip, i) => (
              <li key={i}>{tip}</li>
            ))}
          </ul>
          {season.avoid_tips && (
            <div className="season-avoid">
              <span style={{ fontWeight: 700 }}>⚠️ 避坑：</span>
              {season.avoid_tips}
            </div>
          )}
          {bestVenues.length > 0 && (
            <div className="season-best">
              <div className="season-best-title">🌟 推荐场馆</div>
              <div className="season-best-grid">
                {bestVenues.map(v => (
                  <button
                    key={v.id}
                    className="season-best-tile"
                    onClick={() => navigate(`/venue/${v.id}`)}
                  >
                    <span className="sbt-emoji">{VENUE_EMOJIS[v.id] || '🏠'}</span>
                    <span className="sbt-name">{v.name}</span>
                  </button>
                ))}
              </div>
            </div>
          )}
        </div>
      </div>
    )
  }

  return (
    <div className="fullscreen-flow">
      <header className="flow-header">
        <button className="flow-back" onClick={() => navigate('/')}>←</button>
        <div className="flow-title">🌤️ 淡旺季攻略</div>
        <div style={{ width: 36 }} />
      </header>

      <div className="flow-body">
        <p style={{ fontSize: 13, color: 'var(--fg-muted)', lineHeight: 1.6, margin: '0 0 14px' }}>
          不同季节来红山，体验完全不同。选对时机，避开人潮，看见动物最活跃的一面。
        </p>

        {!sg ? (
          <div className="card" style={{ textAlign: 'center', color: 'var(--fg-muted)', padding: 30 }}>
            加载中…
          </div>
        ) : (
          <>
            {renderSeason('peak')}
            {renderSeason('shoulder')}
            {renderSeason('off_peak')}

            <div className="card" style={{ background: '#fef9e7' }}>
              <div className="vd-section-title">💡 通用建议</div>
              <ul className="season-tips-list" style={{ color: '#78350f' }}>
                {(meta.tips || []).map((t, i) => (
                  <li key={i}>{t}</li>
                ))}
              </ul>
            </div>
          </>
        )}
      </div>
    </div>
  )
}
