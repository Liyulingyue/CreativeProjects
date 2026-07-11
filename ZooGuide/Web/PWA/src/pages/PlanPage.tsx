import { useState } from 'react'
import type { Route, UserPreference, Venue } from '../types'
import { api } from '../api/client'
import { Home } from '../components/Home'
import { Questionnaire } from '../components/Questionnaire'
import { RouteView } from '../components/RouteView'
import { VariantsModal } from '../components/VariantsModal'
import { ChatDialog } from '../components/ChatDialog'

interface Props {
  initialPrefs: UserPreference | null
  venues: Venue[]
  meta: any
  user: any
}

type Stage = 'home' | 'quiz' | 'loading' | 'route' | 'error'

export function PlanPage({ initialPrefs, venues, meta, user }: Props) {
  const [stage, setStage] = useState<Stage>('home')
  const [prefs, setPrefs] = useState<UserPreference | null>(initialPrefs)
  const [route, setRoute] = useState<Route | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [fastMode, setFastMode] = useState(false)
  const [strictHours, setStrictHours] = useState(false)
  const [chatOpen, setChatOpen] = useState(false)
  const [variantsOpen, setVariantsOpen] = useState(false)
  const [settingsOpen, setSettingsOpen] = useState(false)

  async function handlePlan(p: UserPreference) {
    setPrefs(p)
    setStage('loading')
    setError(null)
    try {
      const r = await api.plan({ ...p, fast: fastMode, strict_hours: strictHours })
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

  function pickVariant(r: Route) {
    setRoute(r)
    setStage('route')
  }

  return (
    <div>
      {stage === 'home' && (
        <>
          <Home onStart={startQuiz} />
          {meta && (
            <div className="card" style={{ marginTop: 14 }}>
              <h3 className="card-title">📋 园区速览</h3>
              <div className="meta-info">
                <div className="item">🕒 {meta.open_time}–{meta.close_time}</div>
                <div className="item">🎫 {meta.ticket}</div>
                <div className="item">📍 {venues.length} 个展馆</div>
                <div className="item">📐 {Object.keys(meta.areas).length} 大片区</div>
              </div>
            </div>
          )}
          <button
            className="btn btn-outline btn-full"
            style={{ marginTop: 14 }}
            onClick={() => setSettingsOpen(true)}
          >
            ⚙️ 规划设置
          </button>
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
        <>
          <RouteView
            route={route}
            prefs={prefs}
            onRouteUpdate={setRoute}
            onReset={reset}
            onChat={() => setChatOpen(true)}
            onVariants={() => setVariantsOpen(true)}
          />
        </>
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

      {settingsOpen && (
        <SettingsModal
          fastMode={fastMode}
          strictHours={strictHours}
          onChange={(s) => {
            setFastMode(s.fastMode)
            setStrictHours(s.strictHours)
          }}
          onClose={() => setSettingsOpen(false)}
        />
      )}
      {chatOpen && (
        <ChatDialog
          onClose={() => setChatOpen(false)}
          currentRoute={route}
          prefs={prefs}
          onNewRoute={(r) => setRoute(r)}
        />
      )}
      {variantsOpen && prefs && (
        <VariantsModal prefs={prefs} onClose={() => setVariantsOpen(false)} onPick={pickVariant} />
      )}
    </div>
  )
}

function SettingsModal({
  fastMode,
  strictHours,
  onChange,
  onClose,
}: {
  fastMode: boolean
  strictHours: boolean
  onChange: (s: { fastMode: boolean; strictHours: boolean }) => void
  onClose: () => void
}) {
  return (
    <div className="modal-mask" onClick={onClose}>
      <div className="modal" onClick={(e) => e.stopPropagation()}>
        <h3>⚙️ 规划设置</h3>

        <label
          className={`qz-toggle ${fastMode ? 'on' : ''}`}
          style={{ marginTop: 14, cursor: 'pointer' }}
          onClick={() => onChange({ fastMode: !fastMode, strictHours })}
        >
          <div>
            <div className="qz-toggle-label">⚡ 极速模式</div>
            <div className="qz-toggle-desc">跳过 LLM，1-2 秒出方案</div>
          </div>
          <div style={{ fontSize: 22 }}>{fastMode ? '🟢' : '⚪'}</div>
        </label>

        <label
          className={`qz-toggle ${strictHours ? 'on' : ''}`}
          style={{ marginTop: 10, cursor: 'pointer' }}
          onClick={() => onChange({ fastMode, strictHours: !strictHours })}
        >
          <div>
            <div className="qz-toggle-label">🕒 严格开闭馆</div>
            <div className="qz-toggle-desc">跳过已闭馆的场馆</div>
          </div>
          <div style={{ fontSize: 22 }}>{strictHours ? '🟢' : '⚪'}</div>
        </label>

        <div className="modal-actions">
          <button className="btn btn-primary btn-full" onClick={onClose}>
            完成
          </button>
        </div>
      </div>
    </div>
  )
}