import { useEffect, useState } from 'react'
import { useNavigate } from 'react-router-dom'
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

export function FacilityListPage() {
  const navigate = useNavigate()
  const [facilities, setFacilities] = useState<Facility[]>([])
  const [categories, setCategories] = useState<string[]>([])
  const [activeCategory, setActiveCategory] = useState<string | null>(null)
  const [loading, setLoading] = useState(true)

  useEffect(() => {
    setLoading(true)
    api
      .facilities(activeCategory || undefined)
      .then((d) => {
        setFacilities(d.facilities)
        if (!categories.length) setCategories(d.categories)
      })
      .catch(() => {})
      .finally(() => setLoading(false))
  }, [activeCategory])

  return (
    <div className="fullscreen-flow">
      <header className="flow-header">
        <button className="flow-back" onClick={() => navigate('/')}>←</button>
        <div className="flow-title">🚻 设施信息</div>
        <div style={{ width: 36 }} />
      </header>

      <div className="flow-body">
        <div style={{ display: 'flex', gap: 6, flexWrap: 'wrap', marginBottom: 14 }}>
          <button
            className={`facility-tab ${!activeCategory ? 'active' : ''}`}
            onClick={() => setActiveCategory(null)}
          >
            全部
          </button>
          {categories.map((cat) => (
            <button
              key={cat}
              className={`facility-tab ${activeCategory === cat ? 'active' : ''}`}
              onClick={() => setActiveCategory(cat === activeCategory ? null : cat)}
            >
              {CATEGORY_ICONS[cat] || '📍'} {cat}
            </button>
          ))}
        </div>

        {loading ? (
          <div style={{ textAlign: 'center', padding: 40, color: 'var(--fg-muted)' }}>加载中…</div>
        ) : (
          <div className="facility-list">
            {facilities.map((f) => (
              <button
                key={f.id}
                className="facility-card"
                onClick={() => navigate(`/facility/${f.id}`)}
              >
                <div className="facility-card-icon">
                  {CATEGORY_ICONS[f.category] || '📍'}
                </div>
                <div className="facility-card-body">
                  <div className="facility-card-name">{f.name}</div>
                  <div className="facility-card-meta">
                    <span>{f.area}</span>
                    {f.open_time && <span> · {f.open_time}</span>}
                  </div>
                  {f.tags.length > 0 && (
                    <div className="facility-card-tags">
                      {f.tags.slice(0, 3).map((t) => (
                        <span key={t} className="facility-tag">{t}</span>
                      ))}
                    </div>
                  )}
                </div>
                <div className="facility-card-arrow">›</div>
              </button>
            ))}
          </div>
        )}
      </div>
    </div>
  )
}
