import React from 'react'

type Mode = 'word' | 'article'

export default function ModeSlider({ value, onChange }: { value: Mode; onChange: (v: Mode) => void }) {
  const v = value
  const i = v === 'word' ? 0 : 1
  const handle = (e: React.ChangeEvent<HTMLInputElement>) => {
    const next: Mode = (e.target.value === '0' ? 'word' : 'article') as Mode
    onChange(next)
  }
  return (
    <div className="mode-slider" aria-label="切换模式" title="单词模式 / 短文模式">
      <input
        type="range"
        min={0}
        max={1}
        step={1}
        value={i}
        onChange={handle}
        className="mode-slider-input"
      />
      <div className="mode-slider-labels" aria-hidden>
        <span className={v === 'word' ? 'active' : ''}>单词模式</span>
        <span className={v === 'article' ? 'active' : ''}>短文模式</span>
      </div>
    </div>
  )
}
