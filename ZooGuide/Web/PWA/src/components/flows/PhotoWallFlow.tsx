import { useState } from 'react'
import { loadPhotoLog, type PhotoLogEntry } from '../../lib/storage'

interface Props {
  onClose: () => void
  onOpenPhoto: () => void
}

export function PhotoWallFlow({ onClose, onOpenPhoto }: Props) {
  const [log] = useState<PhotoLogEntry[]>(loadPhotoLog())
  const [filter, setFilter] = useState<'all' | 'high' | 'today'>('all')

  const today = new Date().toDateString()
  const filtered = log.filter((p) => {
    if (filter === 'high') return p.vibe_score >= 80
    if (filter === 'today') return new Date(p.ts).toDateString() === today
    return true
  })

  const maxVibe = log.length > 0 ? Math.max(...log.map((p) => p.vibe_score)) : 0
  const avgVibe =
    log.length > 0 ? Math.round(log.reduce((s, p) => s + p.vibe_score, 0) / log.length) : 0

  return (
    <div className="fullscreen-flow">
      <header className="flow-header">
        <button className="flow-back" onClick={onClose}>
          ←
        </button>
        <div className="flow-title">🌟 出片墙</div>
        <button
          className="flow-back"
          onClick={onOpenPhoto}
          style={{ background: 'rgba(255,255,255,0.18)' }}
          title="拍一张"
        >
          📷
        </button>
      </header>

      <div className="flow-body">
        {/* 顶部统计 */}
        <div
          style={{
            display: 'grid',
            gridTemplateColumns: '1fr 1fr 1fr',
            gap: 8,
            marginBottom: 14,
          }}
        >
          <StatBox label="总张数" value={log.length} />
          <StatBox label="最高分" value={maxVibe} />
          <StatBox label="平均分" value={avgVibe} />
        </div>

        {/* 筛选 chips */}
        <div
          style={{
            display: 'flex',
            gap: 6,
            marginBottom: 14,
          }}
        >
          {[
            { key: 'all', label: '全部' },
            { key: 'high', label: '高分 80+' },
            { key: 'today', label: '今天' },
          ].map((f) => (
            <button
              key={f.key}
              className="chat-quick-chip"
              style={
                filter === f.key
                  ? { background: 'var(--primary)', color: 'white', borderColor: 'var(--primary)' }
                  : undefined
              }
              onClick={() => setFilter(f.key as any)}
            >
              {f.label}
            </button>
          ))}
        </div>

        {/* 出片网格 */}
        {filtered.length === 0 ? (
          <div
            className="card"
            style={{
              textAlign: 'center',
              color: 'var(--fg-muted)',
              padding: 40,
            }}
          >
            <div style={{ fontSize: 48, marginBottom: 8 }}>📷</div>
            还没有出片
            <div style={{ fontSize: 12, marginTop: 6 }}>
              {filter === 'today' ? '今天还没拍' : filter === 'high' ? '还没有 80+ 分的出片' : '去拍第一张'}
            </div>
            <button
              className="btn btn-primary"
              style={{ marginTop: 14 }}
              onClick={onOpenPhoto}
            >
              📷 来一张
            </button>
          </div>
        ) : (
          <div
            style={{
              display: 'grid',
              gridTemplateColumns: '1fr 1fr',
              gap: 10,
            }}
          >
            {filtered.map((p) => (
              <div key={p.evaluation_id} className="wall-photo-card">
                <div className="wall-photo-emoji">📷</div>
                <div className="wall-photo-venue">{p.matched_venue_name}</div>
                <div className="wall-photo-animal">{p.animal_guess}</div>
                <div className="wall-photo-row">
                  <span className="wall-photo-badge">{p.badge}</span>
                  <span className="wall-photo-score">{p.vibe_score}分</span>
                </div>
                <div className="wall-photo-caption">「{p.caption}」</div>
                <div className="wall-photo-time">
                  {new Date(p.ts).toLocaleString('zh-CN', {
                    month: 'numeric',
                    day: 'numeric',
                    hour: '2-digit',
                    minute: '2-digit',
                  })}
                </div>
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  )
}

function StatBox({ label, value }: { label: string; value: number }) {
  return (
    <div className="activity-stat-cell">
      <div className="activity-stat-num">{value}</div>
      <div className="activity-stat-label">{label}</div>
    </div>
  )
}