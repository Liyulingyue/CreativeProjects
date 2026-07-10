import { useEffect, useState } from 'react'
import { api } from './api/client'
import type { Meta, Route, UserPreference, Venue } from './types'
import { Home } from './components/Home'
import { Questionnaire } from './components/Questionnaire'
import { RouteView } from './components/RouteView'
import { AuthModal } from './components/AuthModal'
import { ProfileModal } from './components/ProfileModal'
import { getStoredUser, loadPrefs } from './lib/storage'
import type { AuthUser } from './lib/storage'

type Stage = 'home' | 'quiz' | 'loading' | 'route' | 'error'

export default function App() {
  const [stage, setStage] = useState<Stage>('home')
  const [prefs, setPrefs] = useState<UserPreference | null>(null)
  const [route, setRoute] = useState<Route | null>(null)
  const [meta, setMeta] = useState<Meta | null>(null)
  const [venues, setVenues] = useState<Venue[]>([])
  const [error, setError] = useState<string | null>(null)
  const [fastMode, setFastMode] = useState(false)
  const [authOpen, setAuthOpen] = useState(false)
  const [profileOpen, setProfileOpen] = useState(false)
  const [user, setUser] = useState<AuthUser | null>(getStoredUser())

  useEffect(() => {
    api.meta().then(setMeta).catch(console.error)
    api.venues().then((d) => setVenues(d.venues)).catch(console.error)
    const saved = loadPrefs()
    if (saved) setPrefs(saved)
  }, [])

  async function handlePlan(p: UserPreference) {
    setPrefs(p)
    setStage('loading')
    setError(null)
    try {
      const r = await api.plan({ ...p, fast: fastMode })
      setRoute(r)
      setStage('route')
    } catch (e) {
      setError(e instanceof Error ? e.message : '规划失败')
      setStage('error')
    }
  }

  function reset() {
    setStage('home')
    setRoute(null)
  }

  function startQuiz() {
    setStage('quiz')
  }

  function onAuthed(u: AuthUser) {
    setUser(u)
  }

  return (
    <div className="app">
      <header className="app-header">
        <h1>🦒 ZooGuide</h1>
        <span className="badge">红山省力 Agent</span>
        <button
          onClick={() => (user ? setProfileOpen(true) : setAuthOpen(true))}
          style={{
            background: 'rgba(255,255,255,0.18)',
            color: 'white',
            padding: '4px 10px',
            borderRadius: 8,
            fontSize: 12,
            fontWeight: 600,
          }}
        >
          {user ? `👤 ${user.display_name}` : '登录'}
        </button>
      </header>

      <main className="app-body">
        {stage === 'home' && (
          <>
            <Home onStart={startQuiz} />
            {meta && <MetaInfo meta={meta} venues={venues.length} />}
            <div className="card" style={{ marginTop: 14, background: 'var(--primary-soft)', border: 'none' }}>
              <label style={{ display: 'flex', alignItems: 'center', gap: 10, cursor: 'pointer' }}>
                <input
                  type="checkbox"
                  checked={fastMode}
                  onChange={(e) => setFastMode(e.target.checked)}
                  style={{ width: 18, height: 18, accentColor: 'var(--primary)' }}
                />
                <div>
                  <div style={{ fontWeight: 600, color: 'var(--primary-strong)', fontSize: 14 }}>
                    ⚡ 极速模式（跳过 LLM）
                  </div>
                  <div style={{ fontSize: 11, color: 'var(--fg-muted)', marginTop: 2 }}>
                    1-2 秒出方案，但讲解词通用。LLM 模式 30-90 秒，但讲解个性化
                  </div>
                </div>
              </label>
            </div>
          </>
        )}

        {stage === 'quiz' && (
          <Questionnaire onComplete={handlePlan} initial={prefs} />
        )}

        {stage === 'loading' && (
          <div className="loading">
            <div className="spinner" />
            正在为你定制红山路线…
            <div style={{ fontSize: 12, marginTop: 8 }}>
              规则引擎筛选 + LLM 编排中
            </div>
            <div style={{ fontSize: 11, marginTop: 16, color: 'var(--fg-muted)', maxWidth: 280, lineHeight: 1.5 }}>
              ⏱️ LLM 思考中（一般 30-90 秒）
              <br />
              如果太久，会自动用规则引擎的方案
            </div>
          </div>
        )}

        {stage === 'route' && route && prefs && (
          <RouteView route={route} prefs={prefs} onRouteUpdate={setRoute} onReset={reset} />
        )}

        {stage === 'error' && (
          <>
            <div className="error-banner">
              <strong>规划失败：</strong>{error}
            </div>
            <button className="btn btn-primary btn-full" onClick={() => setStage('quiz')}>
              再试一次
            </button>
          </>
        )}
      </main>

      {authOpen && <AuthModal onClose={() => setAuthOpen(false)} onAuthed={onAuthed} />}
      {profileOpen && (
        <ProfileModal
          onClose={() => setProfileOpen(false)}
          onGoLogin={() => setAuthOpen(true)}
        />
      )}
    </div>
  )
}

function MetaInfo({ meta, venues }: { meta: Meta; venues: number }) {
  return (
    <div className="card" style={{ marginTop: 18 }}>
      <h3 className="card-title">📋 园区速览</h3>
      <div className="meta-info">
        <div className="item">🕒 {meta.open_time}–{meta.close_time}</div>
        <div className="item">🎫 {meta.ticket}</div>
        <div className="item">📍 {venues} 个展馆</div>
        <div className="item">📐 {Object.keys(meta.areas).length} 大片区</div>
      </div>
    </div>
  )
}