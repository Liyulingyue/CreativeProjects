import { useState } from 'react'
import type { UserPreference, PartyType, Gate, InterestTag } from '../types'
import { useQuizOptions } from '../hooks/useQuizOptions'
import { savePrefs } from '../lib/storage'

interface Props {
  onComplete: (prefs: UserPreference) => void
  initial?: UserPreference | null
}

const TOTAL_STEPS = 5

export function Questionnaire({ onComplete, initial }: Props) {
  const opts = useQuizOptions()
  const [step, setStep] = useState(0)

  const [available_hours, setHours] = useState(initial?.available_hours ?? 3)
  const [party_type, setPartyType] = useState<PartyType>(initial?.party_type ?? 'family_young')
  const [with_kids, setWithKids] = useState(initial?.with_kids ?? true)
  const [kids_age, setKidsAge] = useState<number | undefined>(initial?.kids_age ?? 5)
  const [stamina, setStamina] = useState(initial?.stamina ?? 3)
  const [sun_tolerance, setSun] = useState(initial?.sun_tolerance ?? 3)
  const [willing_to_hike, setHike] = useState(initial?.willing_to_hike ?? false)
  const [animal_interests, setInterests] = useState<InterestTag[]>(initial?.animal_interests ?? [])
  const [entry_gate, setGate] = useState<Gate>(initial?.entry_gate ?? 'north')
  const [start_time, setStartTime] = useState(initial?.start_time ?? '09:00')

  if (!opts) {
    return <div className="loading"><div className="spinner" />加载问卷选项…</div>
  }

  function next() {
    if (step < TOTAL_STEPS - 1) setStep(step + 1)
    else {
      const prefs: UserPreference = {
        available_hours,
        party_type,
        with_kids,
        kids_age: with_kids ? kids_age : null,
        stamina,
        sun_tolerance,
        willing_to_hike,
        animal_interests,
        entry_gate,
        start_time,
      }
      savePrefs(prefs)
      onComplete(prefs)
    }
  }

  function back() {
    if (step > 0) setStep(step - 1)
  }

  function toggleInterest(t: InterestTag) {
    setInterests((prev) => (prev.includes(t) ? prev.filter((x) => x !== t) : [...prev, t]))
  }

  return (
    <div>
      <div className="qz-progress">
        {Array.from({ length: TOTAL_STEPS }).map((_, i) => (
          <div key={i} className={`qz-progress-dot ${i <= step ? 'active' : ''}`} />
        ))}
      </div>
      <div className="qz-step">第 {step + 1} / {TOTAL_STEPS} 步</div>

      {step === 0 && (
        <>
          <h2 className="qz-question">你今天准备逛多久？</h2>
          <div className="card">
            <div style={{ display: 'flex', alignItems: 'center', gap: 12 }}>
              <input
                type="range"
                min={1}
                max={8}
                step={0.5}
                value={available_hours}
                onChange={(e) => setHours(parseFloat(e.target.value))}
                style={{ flex: 1, accentColor: 'var(--primary)' }}
              />
              <div style={{ fontSize: 22, fontWeight: 700, color: 'var(--primary-strong)' }}>
                {available_hours}h
              </div>
            </div>
            <div className="qz-slider-desc" style={{ marginTop: 6 }}>
              {available_hours <= 1.5 && '只有一两小时，得挑精华看'}
              {available_hours > 1.5 && available_hours <= 3 && '半天左右，可以逛 3-5 个场馆'}
              {available_hours > 3 && available_hours <= 5 && '大半天，可以比较从容'}
              {available_hours > 5 && '接近全园深度游'}
            </div>
          </div>
        </>
      )}

      {step === 1 && (
        <>
          <h2 className="qz-question">今天和谁一起来？</h2>
          <div className="qz-options">
            {opts.party_types.map((o) => (
              <button
                key={o.value}
                className={`qz-option ${party_type === o.value ? 'selected' : ''}`}
                onClick={() => {
                  setPartyType(o.value as PartyType)
                  if (o.value === 'family_young' || o.value === 'family_teen') {
                    setWithKids(true)
                  } else if (o.value === 'solo' || o.value === 'couple' || o.value === 'seniors') {
                    setWithKids(false)
                  }
                }}
              >
                <div className="qz-option-icon">{o.icon}</div>
                <div className="qz-option-label">{o.label}</div>
                <div className="qz-option-desc">{o.desc}</div>
              </button>
            ))}
          </div>
          {(party_type === 'family_young' || party_type === 'family_teen' || with_kids) && (
            <div className="card">
              <div style={{ fontSize: 14, color: 'var(--primary-strong)', fontWeight: 600, marginBottom: 8 }}>
                孩子年龄
              </div>
              <div className="qz-slider-row">
                <input
                  type="range"
                  min={1}
                  max={16}
                  value={kids_age ?? 5}
                  onChange={(e) => setKidsAge(parseInt(e.target.value))}
                />
                <div className="qz-slider-value">{kids_age ?? 5}</div>
              </div>
              <div className="qz-slider-desc">
                {kids_age !== undefined && kids_age <= 3 && '学龄前：节奏要慢，多休息'}
                {kids_age !== undefined && kids_age > 3 && kids_age <= 6 && '幼儿园：可以多看明星动物'}
                {kids_age !== undefined && kids_age > 6 && kids_age <= 12 && '小学：能听懂小科普'}
                {kids_age !== undefined && kids_age > 12 && '中学：可以加深度讲解'}
              </div>
            </div>
          )}
        </>
      )}

      {step === 2 && (
        <>
          <h2 className="qz-question">你的体力怎么样？</h2>
          <div className="card">
            <div className="qz-slider-row">
              <input
                type="range"
                min={1}
                max={5}
                value={stamina}
                onChange={(e) => setStamina(parseInt(e.target.value))}
              />
              <div className="qz-slider-value">{stamina}</div>
            </div>
            <div className="qz-slider-desc">{opts.stamina_descriptions[String(stamina)]}</div>
          </div>

          <h2 className="qz-question" style={{ marginTop: 24 }}>你能接受爬山吗？</h2>
          <button
            className={`qz-toggle ${willing_to_hike ? 'on' : ''}`}
            onClick={() => setHike(!willing_to_hike)}
          >
            <div>
              <div className="qz-toggle-label">{willing_to_hike ? '可以爬山' : '尽量平地'}</div>
              <div className="qz-toggle-desc">红山是山地型动物园，部分片区需要爬坡</div>
            </div>
            <div style={{ fontSize: 22 }}>{willing_to_hike ? '⛰️' : '🚶'}</div>
          </button>
        </>
      )}

      {step === 3 && (
        <>
          <h2 className="qz-question">你最怕晒还是无所谓？</h2>
          <div className="card">
            <div className="qz-slider-row">
              <input
                type="range"
                min={1}
                max={5}
                value={sun_tolerance}
                onChange={(e) => setSun(parseInt(e.target.value))}
              />
              <div className="qz-slider-value">{sun_tolerance}</div>
            </div>
            <div className="qz-slider-desc">{opts.sun_descriptions[String(sun_tolerance)]}</div>
          </div>

          <h2 className="qz-question" style={{ marginTop: 24 }}>你最想看什么？（可多选）</h2>
          <div className="qz-options" style={{ gridTemplateColumns: '1fr' }}>
            {opts.interests.map((o) => (
              <button
                key={o.value}
                className={`qz-option ${animal_interests.includes(o.value as InterestTag) ? 'selected' : ''}`}
                onClick={() => toggleInterest(o.value as InterestTag)}
                style={{ minHeight: 0, padding: 10 }}
              >
                <div className="qz-option-label">{o.label}</div>
              </button>
            ))}
          </div>
        </>
      )}

      {step === 4 && (
        <>
          <h2 className="qz-question">从哪个门入园？</h2>
          <div className="qz-options">
            {opts.gates.map((o) => (
              <button
                key={o.value}
                className={`qz-option ${entry_gate === o.value ? 'selected' : ''}`}
                onClick={() => setGate(o.value as Gate)}
              >
                <div className="qz-option-label">{o.label}</div>
                <div className="qz-option-desc">{o.desc}</div>
              </button>
            ))}
          </div>
          <h2 className="qz-question" style={{ marginTop: 18 }}>几点入园？</h2>
          <input
            type="time"
            value={start_time}
            onChange={(e) => setStartTime(e.target.value)}
            style={{
              padding: '12px 14px',
              border: '1px solid var(--border)',
              borderRadius: 12,
              background: '#fff',
              fontSize: 16,
              width: '100%',
            }}
          />
        </>
      )}

      <div className="qz-actions">
        {step > 0 && (
          <button className="btn btn-ghost" onClick={back}>
            ← 上一步
          </button>
        )}
        <button className="btn btn-primary" onClick={next}>
          {step < TOTAL_STEPS - 1 ? '下一步 →' : '生成路线 ✨'}
        </button>
      </div>
    </div>
  )
}