import { useEffect, useState } from 'react'
import { api } from './api/client'
import type { Meta, Route, UserPreference, Venue } from './types'
import { Home } from './components/Home'
import { Questionnaire } from './components/Questionnaire'
import { RouteView } from './components/RouteView'
import { loadPrefs } from './lib/storage'

type Stage = 'home' | 'quiz' | 'loading' | 'route' | 'error'

export default function App() {
  const [stage, setStage] = useState<Stage>('home')
  const [prefs, setPrefs] = useState<UserPreference | null>(null)
  const [route, setRoute] = useState<Route | null>(null)
  const [meta, setMeta] = useState<Meta | null>(null)
  const [venues, setVenues] = useState<Venue[]>([])
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    api.meta().then(setMeta).catch(console.error)
    api.venues().then((d) => setVenues(d.venues)).catch(console.error)
    // load saved prefs on start
    const saved = loadPrefs()
    if (saved) setPrefs(saved)
  }, [])

  async function handlePlan(p: UserPreference) {
    setPrefs(p)
    setStage('loading')
    setError(null)
    try {
      const r = await api.plan(p)
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

  return (
    <div className="app">
      <header className="app-header">
        <h1>🦒 ZooGuide</h1>
        <span className="badge">红山省力 Agent</span>
      </header>

      <main className="app-body">
        {stage === 'home' && (
          <>
            <Home onStart={startQuiz} />
            {meta && <MetaInfo meta={meta} venues={venues.length} />}
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