import { useState } from 'react'
import type { Route, UserPreference } from '../types'
import { api } from '../api/client'
import { Questionnaire } from './Questionnaire'
import { RouteView } from './RouteView'
import { ChatDialog } from './ChatDialog'

interface Props {
  initialPrefs: UserPreference | null
  onClose: () => void
  onRouteChange: (r: Route | null) => void
  onOpenChat: () => void
  externalRoute?: Route | null
  initialStage?: Stage
}

type Stage = 'home' | 'quiz' | 'loading' | 'route' | 'error'

export function PlanFlow({
  initialPrefs,
  onClose,
  onRouteChange,
  onOpenChat,
  externalRoute,
  initialStage,
}: Props) {
  const [stage, setStage] = useState<Stage>(
    initialStage || (externalRoute ? 'route' : 'home')
  )
  const [prefs, setPrefs] = useState<UserPreference | null>(initialPrefs)
  const [route, setRoute] = useState<Route | null>(externalRoute || null)
  const [error, setError] = useState<string | null>(null)
  const [fastMode, setFastMode] = useState(false)
  const [strictHours, setStrictHours] = useState(false)
  const [chatOpen, setChatOpen] = useState(false)
  const [settingsOpen, setSettingsOpen] = useState(false)

  async function handlePlan(p: UserPreference) {
    setPrefs(p)
    setStage('loading')
    setError(null)
    try {
      const r = await api.plan({ ...p, fast: fastMode, strict_hours: strictHours })
      setRoute(r)
      onRouteChange(r)
      setStage('route')
    } catch (e) {
      setError(e instanceof Error ? e.message : '规划失败')
      setStage('error')
    }
  }

  function reset() {
    setStage('home')
    setRoute(null)
    onRouteChange(null)
  }

  function startQuiz() {
    setStage('quiz')
  }

  function restartQuiz() {
    setStage('quiz')
  }

  function handleRouteUpdate(r: Route) {
    setRoute(r)
    onRouteChange(r)
  }

  function handleClose() {
    onClose()
  }

  return (
    <div className="fullscreen-flow">
      <header className="flow-header">
        <button className="flow-back" onClick={handleClose}>
          ←
        </button>
        <div className="flow-title">
          {stage === 'route' ? '🧭 我的路线' : '🧭 定制路线'}
        </div>
        {stage === 'route' && (
          <button
            className="flow-settings"
            onClick={() => setSettingsOpen(true)}
            title="设置"
          >
            ⚙️
          </button>
        )}
      </header>

      <div className="flow-body">
        {stage === 'home' && (
          <div className="flow-home">
            <div className="flow-hero">🦒</div>
            <h2 style={{ margin: '12px 0 6px', color: 'var(--primary-strong)' }}>
              逛红山，不必人挤人
            </h2>
            <p style={{ fontSize: 14, color: 'var(--fg-muted)', lineHeight: 1.6, margin: '0 0 18px' }}>
              告诉我你的时间、体力、带没带娃、怕不怕晒，
              我帮你定制一趟只属于你的红山路线。
            </p>
            <button className="btn btn-primary btn-full" onClick={startQuiz}>
              开始定制路线 ✨
            </button>
            <button
              className="btn btn-ghost btn-full"
              style={{ marginTop: 10 }}
              onClick={() => setSettingsOpen(true)}
            >
              ⚙️ 规划设置
            </button>
          </div>
        )}

        {stage === 'quiz' && (
          <Questionnaire onComplete={handlePlan} initial={prefs} />
        )}

        {stage === 'loading' && (
          <div className="loading">
            <div className="spinner" />
            正在为你定制红山路线…
            <div style={{ fontSize: 12, marginTop: 8 }}>规则引擎筛选 + LLM 编排中</div>
            <div
              style={{
                fontSize: 11,
                marginTop: 16,
                color: 'var(--fg-muted)',
                maxWidth: 280,
                lineHeight: 1.5,
              }}
            >
              ⏱️ LLM 思考中（一般 30-90 秒）
              <br />
              如果太久，会自动用规则引擎的方案
            </div>
          </div>
        )}

        {stage === 'route' && route && prefs && (
          <RouteView
            route={route}
            prefs={prefs}
            onRouteUpdate={handleRouteUpdate}
            onRestartQuiz={restartQuiz}
            onOpenChat={() => {
              setChatOpen(true)
              onOpenChat()
            }}
          />
        )}

        {stage === 'error' && (
          <>
            <div className="error-banner">
              <strong>规划失败：</strong>
              {error}
            </div>
            <button className="btn btn-primary btn-full" onClick={() => setStage('quiz')}>
              再试一次
            </button>
          </>
        )}
      </div>

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
          onNewRoute={(r) => handleRouteUpdate(r)}
        />
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