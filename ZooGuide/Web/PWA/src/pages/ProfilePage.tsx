import { useEffect, useState } from 'react'
import { api } from '../api/client'
import { AuthModal } from '../components/AuthModal'
import { PlanFlow } from '../components/PlanFlow'
import { getStoredUser, clearAuth, type AuthUser } from '../lib/storage'
import type { Route } from '../types'

interface Props {
  user: AuthUser | null
  onUserChange: (u: AuthUser | null) => void
  onRouteOpen?: (r: Route) => void
}

interface Summary {
  user: { id: number; username: string; display_name: string }
  stats: {
    checkins_count: number
    venues_visited: number
    routes_planned: number
    photos_evaluated: number
  }
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

interface Achievement {
  id: string
  name: string
  description: string
  icon: string
  category: string
  criteria_type: string
  criteria_threshold: number
  earned: boolean
  progress: number
  current_value: number
  earned_at: string | null
}

export function ProfilePage({ user, onUserChange, onRouteOpen }: Props) {
  const [summary, setSummary] = useState<Summary | null>(null)
  const [achievements, setAchievements] = useState<Achievement[]>([])
  const [earnedCount, setEarnedCount] = useState(0)
  const [loading, setLoading] = useState(false)
  const [authOpen, setAuthOpen] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [planOpen, setPlanOpen] = useState(false)

  async function load() {
    if (!user) return
    setLoading(true)
    setError(null)
    try {
      const [s, a] = await Promise.all([api.mySummary(), api.myAchievements()])
      setSummary(s)
      setAchievements(a.achievements)
      setEarnedCount(a.earned_count)
    } catch (e) {
      setError(e instanceof Error ? e.message : '加载失败')
    } finally {
      setLoading(false)
    }
  }

  useEffect(() => {
    load()
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [user?.id])

  async function logout() {
    await api.logout().catch(() => {})
    clearAuth()
    onUserChange(null)
    setSummary(null)
  }

  // Login prompt
  if (!user) {
    return (
      <div>
        <div
          className="card"
          style={{
            textAlign: 'center',
            padding: 32,
            background: 'linear-gradient(135deg, var(--primary-soft), #fff)',
          }}
        >
          <div style={{ fontSize: 56, marginBottom: 12 }}>🦒</div>
          <h3 style={{ margin: '0 0 8px', color: 'var(--primary-strong)' }}>登录后开启个人体验</h3>
          <p style={{ fontSize: 13, color: 'var(--fg-muted)', margin: '0 0 18px' }}>
            路线、打卡、照片评价跨设备同步
          </p>
          <button className="btn btn-primary btn-full" onClick={() => setAuthOpen(true)}>
            登录 / 注册
          </button>
        </div>

        <div className="card" style={{ marginTop: 14 }}>
          <h3 className="card-title">🌟 试试这些</h3>
          <ul style={{ paddingLeft: 18, fontSize: 13, color: 'var(--fg)', lineHeight: 1.8 }}>
            <li>注册一个账号</li>
            <li>规划一条路线</li>
            <li>打卡你看到的第一个馆</li>
            <li>拍张照让 Agent 给你打分</li>
          </ul>
        </div>

{authOpen && (
        <AuthModal
          onClose={() => setAuthOpen(false)}
          onAuthed={(u) => {
            onUserChange(u)
            setAuthOpen(false)
          }}
        />
      )}

      {planOpen && (
        <PlanFlow
          initialPrefs={null}
          externalRoute={null}
          onClose={() => setPlanOpen(false)}
          onRouteChange={() => {}}
          onOpenChat={() => {}}
        />
      )}
    </div>
  )
}

  // Logged in view
  return (
    <div>
      <div className="card" style={{ background: 'linear-gradient(135deg, var(--primary-soft), #fff)' }}>
        <div style={{ display: 'flex', alignItems: 'center', gap: 12 }}>
          <div
            style={{
              width: 48,
              height: 48,
              borderRadius: '50%',
              background: 'var(--primary)',
              color: 'white',
              display: 'flex',
              alignItems: 'center',
              justifyContent: 'center',
              fontSize: 22,
            }}
          >
            👤
          </div>
          <div style={{ flex: 1 }}>
            <div style={{ fontSize: 17, fontWeight: 700, color: 'var(--primary-strong)' }}>
              {user.display_name}
            </div>
            <div style={{ fontSize: 12, color: 'var(--fg-muted)' }}>@{user.username}</div>
          </div>
          <button
            onClick={logout}
            style={{
              fontSize: 12,
              padding: '6px 10px',
              borderRadius: 8,
              background: 'transparent',
              border: '1px solid var(--border)',
              color: 'var(--fg-muted)',
            }}
          >
            退出
          </button>
        </div>
      </div>

      {loading && (
        <div className="loading">
          <div className="spinner" />
          加载中…
        </div>
      )}

      {error && <div className="error-banner">{error}</div>}

      {summary && (
        <>
          <div
            style={{
              display: 'grid',
              gridTemplateColumns: '1fr 1fr 1fr 1fr',
              gap: 6,
              marginBottom: 14,
            }}
          >
            <StatBlock label="打卡" value={summary.stats.checkins_count} />
            <StatBlock label="馆" value={summary.stats.venues_visited} />
            <StatBlock label="路线" value={summary.stats.routes_planned} />
            <StatBlock label="出片" value={summary.stats.photos_evaluated} />
          </div>

          {/* 活动成就 */}
          {achievements.length > 0 && (
            <Section
              title={
                <span>
                  🏆 活动成就
                  <span
                    style={{
                      fontSize: 11,
                      color: 'var(--fg-muted)',
                      fontWeight: 500,
                      background: 'var(--bg)',
                      padding: '2px 8px',
                      borderRadius: 999,
                      marginLeft: 8,
                    }}
                  >
                    {earnedCount} / {achievements.length}
                  </span>
                </span>
              }
            >
              {achievements.map((a) => (
                <div
                  key={a.id}
                  className={`history-row achievement-row ${a.earned ? 'earned' : ''}`}
                >
                  <div style={{ display: 'flex', alignItems: 'center', gap: 10 }}>
                    <div
                      style={{
                        fontSize: 24,
                        opacity: a.earned ? 1 : 0.35,
                        filter: a.earned ? 'none' : 'grayscale(1)',
                      }}
                    >
                      {a.icon}
                    </div>
                    <div style={{ flex: 1, minWidth: 0 }}>
                      <div
                        className="history-title"
                        style={{
                          color: a.earned ? 'var(--primary-strong)' : 'var(--fg-muted)',
                        }}
                      >
                        {a.name}
                      </div>
                      <div className="history-meta" style={{ fontSize: 11 }}>
                        {a.description}
                      </div>
                      {!a.earned && a.progress > 0 && (
                        <div
                          style={{
                            marginTop: 4,
                            height: 4,
                            background: 'var(--bg)',
                            borderRadius: 2,
                            overflow: 'hidden',
                          }}
                        >
                          <div
                            style={{
                              height: '100%',
                              width: `${a.progress}%`,
                              background: 'var(--primary)',
                              borderRadius: 2,
                            }}
                          />
                        </div>
                      )}
                    </div>
                    {a.earned ? (
                      <span
                        style={{
                          fontSize: 11,
                          color: 'var(--accent)',
                          fontWeight: 700,
                        }}
                      >
                        ✓
                      </span>
                    ) : (
                      <span
                        style={{
                          fontSize: 11,
                          color: 'var(--fg-muted)',
                        }}
                      >
                        🔒
                      </span>
                    )}
                  </div>
                </div>
              ))}
            </Section>
          )}

          {summary.recent_routes.length > 0 && (
            <Section title="🧭 最近规划的路线">
              {summary.recent_routes.map((r) => (
                <div
                  key={r.id}
                  className="history-row"
                  style={{ cursor: 'pointer' }}
                  onClick={() => setPlanOpen(true)}
                >
                  <div style={{ display: 'flex', alignItems: 'center', gap: 10 }}>
                    <div style={{ flex: 1 }}>
                      <div className="history-title">{r.summary?.slice(0, 40) || '路线'}</div>
                      <div className="history-meta">
                        {Math.round(r.total_minutes / 60 * 10) / 10}h ·{' '}
                        {r.created_at?.slice(0, 10)}
                      </div>
                    </div>
                    <span style={{ fontSize: 18, color: 'var(--primary)' }}>›</span>
                  </div>
                </div>
              ))}
            </Section>
          )}

          {summary.recent_checkins.length > 0 && (
            <Section title="🦒 最近打卡">
              {summary.recent_checkins.map((c, i) => (
                <div key={i} className="history-row">
                  <div className="history-title">{c.venue_name}</div>
                  <div className="history-meta">
                    {c.ts?.slice(0, 16).replace('T', ' ')}
                  </div>
                </div>
              ))}
            </Section>
          )}

          {summary.recent_photos.length > 0 && (
            <Section title="📸 最近的出片">
              {summary.recent_photos.map((p) => (
                <div key={p.evaluation_id} className="history-row">
                  <div className="history-title">
                    🏅 {p.badge} · {p.matched_venue_name || p.animal_guess}
                  </div>
                  <div className="history-meta">
                    {p.vibe_score}分 · {p.ts?.slice(0, 16).replace('T', ' ')}
                  </div>
                </div>
              ))}
            </Section>
          )}

          {summary.recent_routes.length === 0 &&
            summary.recent_checkins.length === 0 &&
            summary.recent_photos.length === 0 &&
            achievements.length === 0 && (
              <div
                className="card"
                style={{ textAlign: 'center', color: 'var(--fg-muted)' }}
              >
                <div style={{ fontSize: 32, marginBottom: 8 }}>🌱</div>
                还没有数据
                <div style={{ fontSize: 12, marginTop: 4 }}>
                  去「规划」逛一次红山，留下第一条记录
                </div>
              </div>
            )}
        </>
      )}
    </div>
  )
}

function StatBlock({ label, value }: { label: string; value: number }) {
  return (
    <div
      style={{
        background: 'var(--primary-soft)',
        padding: '14px 12px',
        borderRadius: 12,
        textAlign: 'center',
      }}
    >
      <div style={{ fontSize: 26, fontWeight: 700, color: 'var(--primary-strong)' }}>
        {value}
      </div>
      <div style={{ fontSize: 11, color: 'var(--fg-muted)' }}>{label}</div>
    </div>
  )
}

function Section({ title, children }: { title: React.ReactNode; children: React.ReactNode }) {
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