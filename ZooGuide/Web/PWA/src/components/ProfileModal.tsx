import { useEffect, useState } from 'react'
import { api } from '../api/client'
import { getStoredUser } from '../lib/storage'

interface Props {
  onClose: () => void
  onGoLogin: () => void
}

interface Summary {
  user: { id: number; username: string; display_name: string }
  stats: { checkins_count: number; venues_visited: number; routes_planned: number; photos_evaluated: number }
  recent_checkins: Array<{ venue_id: string; venue_name: string; ts: string }>
  recent_routes: Array<{ id: string; summary: string; total_minutes: number; created_at: string }>
  recent_photos: Array<{
    evaluation_id: string
    ts: string
    badge: string
    animal_guess: string
    matched_venue_name: string
    vibe_score: number
  }>
}

export function ProfileModal({ onClose, onGoLogin }: Props) {
  const [summary, setSummary] = useState<Summary | null>(null)
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    api
      .mySummary()
      .then(setSummary)
      .catch((e) => setError(e instanceof Error ? e.message : '加载失败'))
      .finally(() => setLoading(false))
  }, [])

  const user = getStoredUser()
  if (!user) {
    return (
      <div className="modal-mask" onClick={onClose}>
        <div className="modal">
          <h3>👤 我的</h3>
          <p style={{ fontSize: 13, color: 'var(--fg-muted)', margin: '0 0 16px' }}>
            登录后可以保存路线、打卡历史、照片评价，下次打开还在。
          </p>
          <div className="modal-actions">
            <button className="btn btn-ghost" onClick={onClose}>
              关闭
            </button>
            <button
              className="btn btn-primary"
              onClick={() => {
                onClose()
                onGoLogin()
              }}
            >
              登录 / 注册
            </button>
          </div>
        </div>
      </div>
    )
  }

  return (
    <div className="modal-mask" onClick={onClose}>
      <div className="modal" onClick={(e) => e.stopPropagation()}>
        <h3>👤 {user.display_name}</h3>
        <p style={{ fontSize: 11, color: 'var(--fg-muted)', margin: '0 0 12px' }}>
          @{user.username}
        </p>

        {loading && (
          <div className="loading">
            <div className="spinner" />
            加载历史…
          </div>
        )}

        {error && <div className="error-banner">{error}</div>}

        {summary && (
          <>
            <div
              style={{
                display: 'grid',
                gridTemplateColumns: '1fr 1fr',
                gap: 8,
                marginBottom: 14,
              }}
            >
              <StatCard label="打卡次数" value={summary.stats.checkins_count} />
              <StatCard label="去过的馆" value={summary.stats.venues_visited} />
              <StatCard label="规划路线" value={summary.stats.routes_planned} />
              <StatCard label="照片评价" value={summary.stats.photos_evaluated} />
            </div>

            {summary.recent_routes.length > 0 && (
              <Section title="🧭 最近的路线">
                {summary.recent_routes.map((r) => (
                  <div key={r.id} className="history-row">
                    <div className="history-main">
                      <div className="history-title">{r.summary?.slice(0, 50) || '路线'}</div>
                      <div className="history-meta">
                        {Math.round(r.total_minutes / 60 * 10) / 10}h · {r.created_at?.slice(0, 10)}
                      </div>
                    </div>
                  </div>
                ))}
              </Section>
            )}

            {summary.recent_checkins.length > 0 && (
              <Section title="🦒 最近打卡">
                {summary.recent_checkins.map((c, i) => (
                  <div key={i} className="history-row">
                    <div className="history-main">
                      <div className="history-title">{c.venue_name}</div>
                      <div className="history-meta">{c.ts?.slice(0, 16).replace('T', ' ')}</div>
                    </div>
                  </div>
                ))}
              </Section>
            )}

            {summary.recent_photos.length > 0 && (
              <Section title="📸 最近的出片">
                {summary.recent_photos.map((p) => (
                  <div key={p.evaluation_id} className="history-row">
                    <div className="history-main">
                      <div className="history-title">
                        🏅 {p.badge} · {p.matched_venue_name || p.animal_guess}
                      </div>
                      <div className="history-meta">
                        {p.vibe_score}分 · {p.ts?.slice(0, 16).replace('T', ' ')}
                      </div>
                    </div>
                  </div>
                ))}
              </Section>
            )}
          </>
        )}

        <div className="modal-actions">
          <button
            className="btn btn-ghost"
            onClick={async () => {
              await api.logout().catch(() => {})
              clearAuth()
              onClose()
            }}
          >
            退出登录
          </button>
          <button className="btn btn-primary" onClick={onClose}>
            完成
          </button>
        </div>
      </div>
    </div>
  )
}

function StatCard({ label, value }: { label: string; value: number }) {
  return (
    <div
      style={{
        background: 'var(--primary-soft)',
        padding: '10px 12px',
        borderRadius: 10,
        textAlign: 'center',
      }}
    >
      <div style={{ fontSize: 22, fontWeight: 700, color: 'var(--primary-strong)' }}>{value}</div>
      <div style={{ fontSize: 11, color: 'var(--fg-muted)' }}>{label}</div>
    </div>
  )
}

function Section({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <div style={{ marginBottom: 12 }}>
      <div
        style={{
          fontSize: 12,
          color: 'var(--fg-muted)',
          marginBottom: 6,
          fontWeight: 600,
        }}
      >
        {title}
      </div>
      {children}
    </div>
  )
}

// local helper to avoid circular import
function clearAuth() {
  localStorage.removeItem('zooguide:token:v1')
  localStorage.removeItem('zooguide:user:v1')
}