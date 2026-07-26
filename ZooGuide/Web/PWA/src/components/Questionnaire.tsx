import { useState } from 'react'
import type { UserPreference, PartyType, Gate, InterestTag, SliderConfig } from '../types'
import { useQuizOptions } from '../hooks/useQuizOptions'
import { savePrefs } from '../lib/storage'

interface Props {
  onComplete: (prefs: UserPreference) => void
  initial?: UserPreference | null
}

function sliderDesc(slider: SliderConfig, value: number): string {
  const d = slider.descriptions
  if (Array.isArray(d)) {
    for (const t of d) {
      if (value <= t.max) return t.text
    }
    return d[d.length - 1]?.text ?? ''
  }
  return d[String(value)] ?? ''
}

export function Questionnaire({ onComplete, initial }: Props) {
  const opts = useQuizOptions()

  const timeSlider = opts?.sliders?.time
  const staminaSlider = opts?.sliders?.stamina
  const sunSlider = opts?.sliders?.sun_tolerance
  const kidsAgeSlider = opts?.sliders?.kids_age

  const [available_hours, setHours] = useState(initial?.available_hours ?? timeSlider?.default ?? 3)
  const [party_type, setPartyType] = useState<PartyType | null>(initial?.party_type ?? null)
  const [with_kids, setWithKids] = useState(initial?.with_kids ?? false)
  const [kids_age, setKidsAge] = useState<number | undefined>(initial?.kids_age ?? kidsAgeSlider?.default ?? 5)
  const [stamina, setStamina] = useState(initial?.stamina ?? staminaSlider?.default ?? 3)
  const [sun_tolerance, setSun] = useState(initial?.sun_tolerance ?? sunSlider?.default ?? 3)
  const [willing_to_hike, setHike] = useState(initial?.willing_to_hike ?? false)
  const [animal_interests, setInterests] = useState<InterestTag[]>(initial?.animal_interests ?? [])
  const [entry_gate, setGate] = useState<Gate | null>(initial?.entry_gate ?? null)
  const [start_time, setStartTime] = useState(initial?.start_time ?? '09:00')

  if (!opts) {
    return <div className="loading"><div className="spinner" />加载问卷选项…</div>
  }

  const requiredFields = new Set(opts.required_fields ?? [])
  const showKidsAge = opts.conditional_fields?.some(
    (cf) => cf.field === 'kids_age' && cf.show_when === 'with_kids'
  ) && (party_type === 'family_young' || party_type === 'family_teen' || with_kids)

  function canSubmit(): boolean {
    if (requiredFields.has('party_type') && party_type === null) return false
    if (requiredFields.has('entry_gate') && entry_gate === null) return false
    return true
  }

  function selectPartyType(value: PartyType) {
    setPartyType(value)
    const opt = opts!.party_types.find((o) => o.value === value)
    if (opt?.implies_with_kids) {
      setWithKids(true)
    } else if (opt && opt.implies_with_kids === false) {
      setWithKids(false)
    }
  }

  function submit() {
    if (!canSubmit()) return
    const prefs: UserPreference = {
      available_hours,
      party_type: party_type!,
      with_kids,
      kids_age: with_kids ? kids_age : null,
      stamina,
      sun_tolerance,
      willing_to_hike,
      animal_interests,
      entry_gate: entry_gate!,
      start_time,
      fast: false,
    }
    savePrefs(prefs)
    onComplete(prefs)
  }

  function toggleInterest(t: InterestTag) {
    setInterests((prev) => (prev.includes(t) ? prev.filter((x) => x !== t) : [...prev, t]))
  }

  return (
    <div className="qz-form">
      <h2 className="qz-question">你今天准备逛多久？</h2>
      <div className="card">
        <div style={{ display: 'flex', alignItems: 'center', gap: 12 }}>
          <input
            type="range"
            min={timeSlider?.min ?? 1}
            max={timeSlider?.max ?? 8}
            step={timeSlider?.step ?? 0.5}
            value={available_hours}
            onChange={(e) => setHours(parseFloat(e.target.value))}
            style={{ flex: 1, accentColor: 'var(--primary)' }}
            aria-label="游览时长（小时）"
          />
          <div style={{ fontSize: 22, fontWeight: 700, color: 'var(--primary-strong)' }}>
            {available_hours}{timeSlider?.unit ?? 'h'}
          </div>
        </div>
        <div className="qz-slider-desc" style={{ marginTop: 6 }}>
          {timeSlider ? sliderDesc(timeSlider, available_hours) : ''}
        </div>
      </div>

      <h2 className="qz-question">今天和谁一起来？</h2>
      <div className="qz-options">
        {opts.party_types.map((o) => (
          <button
            key={o.value}
            className={`qz-option ${party_type === o.value ? 'selected' : ''}`}
            onClick={() => selectPartyType(o.value as PartyType)}
          >
            <div className="qz-option-icon">{o.icon}</div>
            <div className="qz-option-label">{o.label}</div>
            <div className="qz-option-desc">{o.desc}</div>
          </button>
        ))}
      </div>

      {showKidsAge && kidsAgeSlider && (
        <div className="card">
          <div style={{ fontSize: 14, color: 'var(--primary-strong)', fontWeight: 600, marginBottom: 8 }}>
            孩子年龄
          </div>
          <div className="qz-slider-row">
            <input
              type="range"
              min={kidsAgeSlider.min}
              max={kidsAgeSlider.max}
              step={kidsAgeSlider.step}
              value={kids_age ?? kidsAgeSlider.default}
              onChange={(e) => setKidsAge(parseInt(e.target.value))}
              aria-label="孩子年龄"
            />
            <div className="qz-slider-value">{kids_age ?? kidsAgeSlider.default}</div>
          </div>
          <div className="qz-slider-desc">
            {sliderDesc(kidsAgeSlider, kids_age ?? kidsAgeSlider.default)}
          </div>
        </div>
      )}

      <h2 className="qz-question">你的体力怎么样？</h2>
      <div className="card">
        <div className="qz-slider-row">
          <input
            type="range"
            min={staminaSlider?.min ?? 1}
            max={staminaSlider?.max ?? 5}
            step={staminaSlider?.step ?? 1}
            value={stamina}
            onChange={(e) => setStamina(parseInt(e.target.value))}
            aria-label="体力等级"
          />
          <div className="qz-slider-value">{stamina}</div>
        </div>
        <div className="qz-slider-desc">{staminaSlider ? sliderDesc(staminaSlider, stamina) : ''}</div>
      </div>

      <h2 className="qz-question">你能接受爬山吗？</h2>
      <button
        className={`qz-toggle ${willing_to_hike ? 'on' : ''}`}
        onClick={() => setHike(!willing_to_hike)}
      >
        <div>
          <div className="qz-toggle-label">{opts.hike_options[String(willing_to_hike)] || (willing_to_hike ? '可以爬山' : '尽量平地')}</div>
          <div className="qz-toggle-desc">{opts.hike_terrain_hint}</div>
        </div>
        <div style={{ fontSize: 22 }}>{willing_to_hike ? '⛰️' : '🚶'}</div>
      </button>

      <h2 className="qz-question">你最怕晒还是无所谓？</h2>
      <div className="card">
        <div className="qz-slider-row">
          <input
            type="range"
            min={sunSlider?.min ?? 1}
            max={sunSlider?.max ?? 5}
            step={sunSlider?.step ?? 1}
            value={sun_tolerance}
            onChange={(e) => setSun(parseInt(e.target.value))}
            aria-label="晒太阳接受度"
          />
          <div className="qz-slider-value">{sun_tolerance}</div>
        </div>
        <div className="qz-slider-desc">{sunSlider ? sliderDesc(sunSlider, sun_tolerance) : ''}</div>
      </div>

      <h2 className="qz-question">你最想看什么？（可多选）</h2>
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

      <h2 className="qz-question">几点入园？</h2>
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

      <div className="qz-actions" style={{ marginTop: 24 }}>
        <button className="btn btn-primary btn-full" onClick={submit} disabled={!canSubmit()}>
          生成路线 ✨
        </button>
      </div>
    </div>
  )
}
